import { spawn } from "node:child_process";
import { access, rm } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { resolve } from "node:path";
import { constants } from "node:fs";

const projectRoot = fileURLToPath(new URL("..", import.meta.url));
const binary = process.argv[2]
  ? resolve(process.argv[2])
  : resolve(projectRoot, "target/release/codescope");
const fixture = resolve(process.argv[3] ?? resolve(projectRoot, "tests/fixtures/sample.tsx"));
const statisticsPath = resolve(projectRoot, "target/codescope-smoke-stats.json");

await access(binary, constants.X_OK);
await access(fixture, constants.R_OK);
await rm(statisticsPath, { force: true });

const child = spawn(binary, [], {
  cwd: projectRoot,
  env: {
    ...process.env,
    CODESCOPE_TYPESCRIPT_ROOT: projectRoot,
    CODESCOPE_STATS_PATH: statisticsPath,
  },
  stdio: ["pipe", "pipe", "pipe"],
});

let buffer = "";
let nextId = 1;
const pending = new Map();

function failPending(error) {
  for (const { reject } of pending.values()) reject(error);
  pending.clear();
}

child.stdout.setEncoding("utf8");
child.stdout.on("data", (chunk) => {
  buffer += chunk;
  while (true) {
    const newline = buffer.indexOf("\n");
    if (newline < 0) return;
    const line = buffer.slice(0, newline).trim();
    buffer = buffer.slice(newline + 1);
    if (!line) continue;
    let message;
    try {
      message = JSON.parse(line);
    } catch (error) {
      failPending(new Error(`Invalid MCP response: ${error.message}`));
      return;
    }
    const request = pending.get(message.id);
    if (!request) continue;
    pending.delete(message.id);
    clearTimeout(request.timer);
    if (message.error) request.reject(new Error(JSON.stringify(message.error)));
    else request.resolve(message);
  }
});

let stderr = "";
child.stderr.setEncoding("utf8");
child.stderr.on("data", (chunk) => {
  stderr += chunk;
});
child.on("error", (error) => failPending(error));
child.on("exit", (code, signal) => {
  if (pending.size > 0) {
    failPending(new Error(`MCP server exited (${code ?? signal})${stderr ? `: ${stderr.trim()}` : ""}`));
  }
});

function send(message) {
  child.stdin.write(`${JSON.stringify(message)}\n`);
}

function request(method, params) {
  const id = nextId++;
  return new Promise((resolveResponse, reject) => {
    const timer = setTimeout(() => {
      pending.delete(id);
      reject(new Error(`Timed out waiting for ${method}`));
    }, 10_000);
    pending.set(id, { resolve: resolveResponse, reject, timer });
    send({ jsonrpc: "2.0", id, method, params });
  });
}

try {
  await request("initialize", {
    protocolVersion: "2025-06-18",
    capabilities: {},
    clientInfo: { name: "codescope-release-smoke-test", version: "1.0.0" },
  });
  send({ jsonrpc: "2.0", method: "notifications/initialized" });

  const tools = await request("tools/list", {});
  const toolNames = tools.result.tools.map((tool) => tool.name);
  const requiredTools = ["read_symbol", "list_symbols", "find_dependencies", "read_symbol_context", "get_statistics"];
  for (const name of requiredTools) {
    if (!toolNames.includes(name)) throw new Error(`Missing MCP tool: ${name}`);
  }

  const symbols = await request("tools/call", {
    name: "list_symbols",
    arguments: { path: fixture },
  });
  if (symbols.result.isError) throw new Error("list_symbols returned an MCP error");
  const listed = symbols.result.structuredContent?.symbols ?? [];
  if (!listed.some((symbol) => symbol.name === "sendMessage")) {
    throw new Error("Smoke fixture did not return sendMessage");
  }

  const context = await request("tools/call", {
    name: "read_symbol_context",
    arguments: { path: fixture, symbol: "sendMessage" },
  });
  if (context.result.isError || context.result.structuredContent?.requested_symbol?.symbol !== "sendMessage") {
    throw new Error("read_symbol_context did not return sendMessage");
  }

  const statistics = await request("tools/call", {
    name: "get_statistics",
    arguments: {},
  });
  if (statistics.result.isError || statistics.result.structuredContent?.session?.successful_requests !== 2) {
    throw new Error("get_statistics did not report the successful semantic requests");
  }

  console.log(`MCP release smoke test passed: ${toolNames.join(", ")}`);
} finally {
  child.stdin.end();
  await new Promise((resolveExit) => {
    const timer = setTimeout(() => {
      child.kill("SIGTERM");
      resolveExit();
    }, 2_000);
    child.once("exit", () => {
      clearTimeout(timer);
      resolveExit();
    });
  });
  await rm(statisticsPath, { force: true });
}
