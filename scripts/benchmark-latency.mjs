import { spawn } from "node:child_process";
import { mkdtemp, mkdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const projectRoot = fileURLToPath(new URL("..", import.meta.url));
const binary = resolve(process.argv[2] ?? join(projectRoot, "target/release/symbolpeek"));
const batches = (process.argv[3] ?? "1,10,50")
  .split(",")
  .map(Number)
  .filter((value) => Number.isInteger(value) && value > 0);
const workspace = await mkdtemp(join(tmpdir(), "symbolpeek-latency-"));

const sources = {
  rust: ["rs", (index) => `pub fn target_${index}() -> usize { ${index} }\n`],
  python: ["py", (index) => `def target_${index}():\n    return ${index}\n`],
  java: ["java", (index) => `class Target${index} { int target_${index}() { return ${index}; } }\n`],
  go: ["go", (index) => `package sample\nfunc target_${index}() int { return ${index} }\n`],
};

for (const [language, [extension, source]] of Object.entries(sources)) {
  const directory = join(workspace, language);
  await mkdir(directory, { recursive: true });
  for (let index = 0; index < 40; index += 1) {
    await writeFile(join(directory, `file_${index}.${extension}`), source(index));
  }
}

function percentile(values, ratio) {
  const sorted = [...values].sort((left, right) => left - right);
  return sorted[Math.min(sorted.length - 1, Math.ceil(sorted.length * ratio) - 1)];
}

function summary(name, count, values) {
  const total = values.reduce((sum, value) => sum + value, 0);
  return {
    scenario: name,
    requests: count,
    total_ms: Number(total.toFixed(1)),
    mean_ms: Number((total / count).toFixed(1)),
    p50_ms: Number(percentile(values, 0.5).toFixed(1)),
    p95_ms: Number(percentile(values, 0.95).toFixed(1)),
    max_ms: Number(Math.max(...values).toFixed(1)),
  };
}

function startServer(extraEnv = {}) {
  const child = spawn(binary, [], {
    cwd: projectRoot,
    env: {
      ...process.env,
      SYMBOLPEEK_TYPESCRIPT_ROOT: projectRoot,
      SYMBOLPEEK_STATS_PATH: join(workspace, `stats-${Date.now()}-${Math.random()}.json`),
      ...extraEnv,
    },
    stdio: ["pipe", "pipe", "pipe"],
  });
  child.stdout.setEncoding("utf8");
  child.stderr.setEncoding("utf8");
  let buffer = "";
  let stderr = "";
  let nextId = 1;
  const pending = new Map();
  child.stderr.on("data", (chunk) => { stderr += chunk; });
  child.stdout.on("data", (chunk) => {
    buffer += chunk;
    while (buffer.includes("\n")) {
      const newline = buffer.indexOf("\n");
      const line = buffer.slice(0, newline).trim();
      buffer = buffer.slice(newline + 1);
      if (!line) continue;
      const response = JSON.parse(line);
      const waiter = pending.get(response.id);
      if (!waiter) continue;
      pending.delete(response.id);
      clearTimeout(waiter.timer);
      if (response.error || response.result?.isError) {
        waiter.reject(new Error(JSON.stringify(response.error ?? response.result)));
      } else {
        waiter.resolve(response);
      }
    }
  });
  child.on("exit", (code, signal) => {
    const error = new Error(`server exited: ${code ?? signal}${stderr ? `: ${stderr}` : ""}`);
    for (const waiter of pending.values()) waiter.reject(error);
    pending.clear();
  });

  function request(method, params) {
    const id = nextId;
    nextId += 1;
    return new Promise((resolveRequest, reject) => {
      const timer = setTimeout(() => {
        pending.delete(id);
        reject(new Error(`${method} exceeded 30s`));
      }, 30_000);
      pending.set(id, { resolve: resolveRequest, reject, timer });
      child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", id, method, params })}\n`);
    });
  }

  return {
    async initialize() {
      await request("initialize", {
        protocolVersion: "2025-06-18",
        capabilities: {},
        clientInfo: { name: "symbolpeek-latency", version: "1" },
      });
      child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", method: "notifications/initialized" })}\n`);
    },
    call(name, args) {
      return request("tools/call", { name, arguments: args });
    },
    async close() {
      child.stdin.end();
      await new Promise((done) => {
        const timer = setTimeout(() => {
          child.kill("SIGTERM");
          done();
        }, 2_000);
        child.once("exit", () => {
          clearTimeout(timer);
          done();
        });
      });
    },
  };
}

async function measure(client, name, args, count) {
  const values = [];
  for (let index = 0; index < count; index += 1) {
    const start = performance.now();
    await client.call("search_symbols", args);
    values.push(performance.now() - start);
  }
  return summary(name, count, values);
}

const rows = [];
try {
  const syntax = startServer({ SYMBOLPEEK_NODE: "/definitely/missing/node" });
  await syntax.initialize();
  for (const language of Object.keys(sources)) {
    const args = { path: join(workspace, language), query: "target", max_results: 50 };
    rows.push(await measure(syntax, `${language}-cold`, args, 1));
    for (const count of batches) rows.push(await measure(syntax, `${language}-warm`, args, count));
  }
  await syntax.close();

  const typescript = startServer();
  await typescript.initialize();
  const tsArgs = {
    path: join(projectRoot, "tests/fixtures/navigation"),
    query: "use",
    max_results: 50,
  };
  rows.push(await measure(typescript, "typescript-cold", tsArgs, 1));
  for (const count of batches) rows.push(await measure(typescript, "typescript-warm", tsArgs, count));
  await typescript.close();

  console.table(rows);
  const slowest = Math.max(...rows.map((row) => row.max_ms));
  console.log(`slowest_response_ms=${slowest.toFixed(1)} under_30s=${slowest < 30_000}`);
} finally {
  await rm(workspace, { recursive: true, force: true });
}
