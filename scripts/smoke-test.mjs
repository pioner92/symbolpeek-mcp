import { spawn } from "node:child_process";
import { access, rm } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { resolve } from "node:path";
import { constants } from "node:fs";

const projectRoot = fileURLToPath(new URL("..", import.meta.url));
const binary = process.argv[2]
  ? resolve(process.argv[2])
  : resolve(projectRoot, "target/release/symbolpeek");
const fixture = resolve(process.argv[3] ?? resolve(projectRoot, "tests/fixtures/sample.tsx"));
const navigationRoot = resolve(projectRoot, "tests/fixtures/navigation");
const contractsFixture = resolve(navigationRoot, "contracts.ts");
const diagnosticsFixture = resolve(navigationRoot, "diagnostics.ts");
const statisticsPath = resolve(projectRoot, "target/symbolpeek-smoke-stats.json");

await access(binary, constants.X_OK);
await access(fixture, constants.R_OK);
await rm(statisticsPath, { force: true });

const child = spawn(binary, [], {
  cwd: projectRoot,
  env: {
    ...process.env,
    SYMBOLPEEK_TYPESCRIPT_ROOT: projectRoot,
    SYMBOLPEEK_STATS_PATH: statisticsPath,
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
    clientInfo: { name: "symbolpeek-release-smoke-test", version: "1.0.0" },
  });
  send({ jsonrpc: "2.0", method: "notifications/initialized" });

  const tools = await request("tools/list", {});
  const toolNames = tools.result.tools.map((tool) => tool.name);
  const requiredTools = [
    "read_symbol",
    "list_symbols",
    "find_dependencies",
    "find_references",
    "find_callers",
    "go_to_definition",
    "read_symbol_context",
    "search_symbols",
    "get_type",
    "find_implementations",
    "get_document_outline",
    "find_callees",
    "get_diagnostics",
    "get_call_hierarchy",
    "get_statistics",
  ];
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

  const references = await request("tools/call", {
    name: "find_references",
    arguments: { path: fixture, symbol: "sendMessage" },
  });
  if (references.result.isError || !(references.result.structuredContent?.references?.length > 0)) {
    throw new Error("find_references did not return sendMessage references");
  }

  const callers = await request("tools/call", {
    name: "find_callers",
    arguments: { path: fixture, symbol: "sendMessage" },
  });
  if (callers.result.isError || !(callers.result.structuredContent?.callers?.length > 0)) {
    throw new Error("find_callers did not return sendMessage callers");
  }

  const definition = await request("tools/call", {
    name: "go_to_definition",
    arguments: { path: fixture, line: 37, column: 31 },
  });
  if (definition.result.isError || !definition.result.structuredContent?.definition) {
    throw new Error("go_to_definition did not resolve sendMessage");
  }

  const search = await request("tools/call", {
    name: "search_symbols",
    arguments: { path: navigationRoot, query: "useAuth" },
  });
  if (search.result.isError || !(search.result.structuredContent?.symbols?.length > 0)) {
    throw new Error("search_symbols did not return workspace matches");
  }

  const typeInfo = await request("tools/call", {
    name: "get_type",
    arguments: { path: fixture, line: 15, column: 8 },
  });
  if (typeInfo.result.isError || !typeInfo.result.structuredContent?.display) {
    throw new Error("get_type did not return hover information");
  }

  const implementations = await request("tools/call", {
    name: "find_implementations",
    arguments: { path: contractsFixture, symbol: "Repository" },
  });
  if (implementations.result.isError
    || !(implementations.result.structuredContent?.implementations?.length >= 2)) {
    throw new Error("find_implementations did not return contract implementations");
  }

  const outline = await request("tools/call", {
    name: "get_document_outline",
    arguments: { path: fixture },
  });
  if (outline.result.isError || !(outline.result.structuredContent?.symbols?.length > 0)) {
    throw new Error("get_document_outline did not return symbols");
  }

  const callees = await request("tools/call", {
    name: "find_callees",
    arguments: { path: fixture, symbol: "sendMessage" },
  });
  if (callees.result.isError || !(callees.result.structuredContent?.callees?.length > 0)) {
    throw new Error("find_callees did not return project callees");
  }

  const diagnostics = await request("tools/call", {
    name: "get_diagnostics",
    arguments: { path: diagnosticsFixture },
  });
  if (diagnostics.result.isError || !(diagnostics.result.structuredContent?.diagnostics?.length > 0)) {
    throw new Error("get_diagnostics did not return compiler diagnostics");
  }

  const hierarchy = await request("tools/call", {
    name: "get_call_hierarchy",
    arguments: { path: fixture, symbol: "sendMessage", depth: 2 },
  });
  if (hierarchy.result.isError || !(hierarchy.result.structuredContent?.nodes?.length > 0)) {
    throw new Error("get_call_hierarchy did not return call graph nodes");
  }

  const statistics = await request("tools/call", {
    name: "get_statistics",
    arguments: {},
  });
  if (statistics.result.isError || statistics.result.structuredContent?.session?.successful_requests !== 12) {
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
