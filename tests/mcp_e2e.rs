use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Write},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};

use serde_json::{json, Value};

struct McpClientProcess {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    statistics_path: std::path::PathBuf,
}

static NEXT_STATISTICS_PATH: AtomicU64 = AtomicU64::new(0);

impl McpClientProcess {
    fn start() -> Self {
        let sequence = NEXT_STATISTICS_PATH.fetch_add(1, Ordering::Relaxed);
        let statistics_path = std::env::temp_dir().join(format!(
            "symbolpeek-mcp-e2e-{}-{sequence}.json",
            std::process::id()
        ));
        let mut child = Command::new(env!("CARGO_BIN_EXE_symbolpeek"))
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .env("SYMBOLPEEK_STATS_PATH", &statistics_path)
            .env("SYMBOLPEEK_WORKSPACE_ROOT", env!("CARGO_MANIFEST_DIR"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("MCP server should start");
        let stdin = child.stdin.take().expect("server stdin should be piped");
        let stdout = child.stdout.take().expect("server stdout should be piped");
        Self {
            child,
            stdin: Some(stdin),
            stdout: BufReader::new(stdout),
            statistics_path,
        }
    }

    fn send(&mut self, request: &Value) {
        let stdin = self.stdin.as_mut().expect("server should still be running");
        serde_json::to_writer(&mut *stdin, request).expect("request should serialize");
        stdin
            .write_all(b"\n")
            .expect("request newline should be written");
        stdin.flush().expect("request should be flushed");
    }

    fn send_raw(&mut self, request: &str) {
        let stdin = self.stdin.as_mut().expect("server should still be running");
        stdin
            .write_all(request.as_bytes())
            .expect("raw request should be written");
        stdin
            .write_all(b"\n")
            .expect("request newline should be written");
        stdin.flush().expect("request should be flushed");
    }

    fn receive(&mut self) -> Value {
        let mut line = String::new();
        self.stdout
            .read_line(&mut line)
            .expect("server response should be readable");
        assert!(
            !line.is_empty(),
            "server exited before returning a response"
        );
        serde_json::from_str(&line).expect("server response should be valid JSON")
    }

    fn initialize(&mut self) -> Value {
        self.send(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "symbolpeek-tests", "version": "1.0.0"}
            }
        }));
        let response = self.receive();
        self.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
        response
    }

    fn shutdown(mut self) {
        drop(self.stdin.take());
        let status = self.child.wait().expect("server should shut down");
        assert!(
            status.success(),
            "server should shut down successfully: {status}"
        );
    }
}

impl Drop for McpClientProcess {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.statistics_path);
    }
}

fn call(name: &str, id: u64, arguments: &Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": {"name": name, "arguments": arguments}
    })
}

fn assert_indexed_items(result: &Value, key: &str) {
    assert!(result["files"].is_array());
    assert!(result["truncated"].is_boolean());
    assert!(result[key].as_array().is_some_and(|items| {
        items.iter().all(|item| {
            item["fileIdx"].as_u64().is_some_and(|index| {
                index
                    < result["files"]
                        .as_array()
                        .map_or(0, |files| files.len() as u64)
            }) && item.get("file").is_none()
        })
    }));
}

fn assert_indexed_callees(result: &Value) {
    assert_indexed_items(result, "callees");
    assert!(result["callees"].as_array().is_some_and(|items| {
        items.iter().all(|item| {
            item.get("definition").is_none_or(|definition| {
                definition["fileIdx"].as_u64().is_some_and(|index| {
                    index
                        < result["files"]
                            .as_array()
                            .map_or(0, |files| files.len() as u64)
                }) && definition.get("file").is_none()
            })
        })
    }));
}

fn fixture_path() -> String {
    format!("{}/tests/fixtures/sample.tsx", env!("CARGO_MANIFEST_DIR"))
}

fn navigation_fixture_path() -> String {
    format!(
        "{}/tests/fixtures/navigation/dashboard.tsx",
        env!("CARGO_MANIFEST_DIR")
    )
}

fn navigation_workspace_path() -> String {
    format!("{}/tests/fixtures/navigation", env!("CARGO_MANIFEST_DIR"))
}

fn contracts_fixture_path() -> String {
    format!(
        "{}/tests/fixtures/navigation/contracts.ts",
        env!("CARGO_MANIFEST_DIR")
    )
}

fn diagnostics_fixture_path() -> String {
    format!(
        "{}/tests/fixtures/navigation/diagnostics.ts",
        env!("CARGO_MANIFEST_DIR")
    )
}

#[test]
fn starts_initializes_registers_tools_and_shuts_down() {
    let mut client = McpClientProcess::start();
    let initialization = client.initialize();
    assert_eq!(initialization["id"], 1);
    assert!(initialization["result"]["serverInfo"]["name"].is_string());

    client.send(&json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}}));
    let response = client.receive();
    let tools = response["result"]["tools"]
        .as_array()
        .expect("tools should be listed");
    let names: Vec<_> = tools
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    assert!(names.contains(&"read_symbol"));
    assert!(names.contains(&"list_symbols"));
    assert!(names.contains(&"find_dependencies"));
    assert!(names.contains(&"find_references"));
    assert!(names.contains(&"find_callers"));
    assert!(names.contains(&"go_to_definition"));
    assert!(names.contains(&"read_symbol_context"));
    assert!(names.contains(&"search_symbols"));
    assert!(names.contains(&"get_type"));
    assert!(names.contains(&"find_implementations"));
    assert!(names.contains(&"get_document_outline"));
    assert!(names.contains(&"find_callees"));
    assert!(names.contains(&"get_diagnostics"));
    assert!(names.contains(&"get_call_hierarchy"));
    assert!(names.contains(&"get_statistics"));

    client.shutdown();
}

#[test]
fn handles_cross_file_navigation_requests() {
    let mut client = McpClientProcess::start();
    let _ = client.initialize();
    let path = navigation_fixture_path();

    client.send(&call(
        "find_references",
        40,
        &json!({"path": path, "symbol": "useAuth"}),
    ));
    let references = client.receive();
    let references_structured = &references["result"]["structuredContent"];
    assert!(references_structured["references"]
        .as_array()
        .is_some_and(|items| items.len() >= 3));
    assert_indexed_items(references_structured, "references");

    client.send(&call(
        "find_references",
        43,
        &json!({"path": path, "symbol": "useAuth", "max_results": 1}),
    ));
    let limited_references = client.receive();
    assert_eq!(
        limited_references["result"]["structuredContent"]["references"]
            .as_array()
            .map_or(0, Vec::len),
        1
    );
    assert_eq!(
        limited_references["result"]["structuredContent"]["truncated"],
        true
    );

    client.send(&call(
        "find_callers",
        41,
        &json!({"path": navigation_fixture_path(), "symbol": "useAuth"}),
    ));
    let callers = client.receive();
    let callers_structured = &callers["result"]["structuredContent"];
    assert!(callers_structured["callers"]
        .as_array()
        .is_some_and(|items| { items.iter().any(|item| item["caller"] == "Dashboard") }));
    assert_indexed_items(callers_structured, "callers");

    client.send(&call(
        "go_to_definition",
        42,
        &json!({"path": navigation_fixture_path(), "line": 5, "column": 20}),
    ));
    let definition = client.receive();
    assert!(
        definition["result"]["structuredContent"]["definition"]["file"]
            .as_str()
            .is_some_and(|file| file.ends_with("navigation/auth.ts"))
    );

    client.shutdown();
}

#[test]
#[allow(clippy::too_many_lines)]
fn handles_ast_intelligence_requests() {
    let mut client = McpClientProcess::start();
    let _ = client.initialize();

    client.send(&call(
        "search_symbols",
        60,
        &json!({"path": navigation_workspace_path(), "query": "useAuth"}),
    ));
    let search = client.receive();
    let search_structured = &search["result"]["structuredContent"];
    assert!(search_structured["symbols"]
        .as_array()
        .is_some_and(|items| items.iter().any(|item| item["name"] == "useAuth")));
    assert_indexed_items(search_structured, "symbols");

    client.send(&call(
        "get_type",
        61,
        &json!({"path": navigation_fixture_path(), "line": 5, "column": 20}),
    ));
    let type_info = client.receive();
    assert!(type_info["result"]["structuredContent"]["display"]
        .as_str()
        .is_some_and(|display| display.contains("useAuth")));

    client.send(&call(
        "find_implementations",
        62,
        &json!({"path": contracts_fixture_path(), "symbol": "Repository"}),
    ));
    let implementations = client.receive();
    let implementations_structured = &implementations["result"]["structuredContent"];
    assert!(implementations_structured["implementations"]
        .as_array()
        .is_some_and(|items| items.len() >= 2));
    assert_indexed_items(implementations_structured, "implementations");

    client.send(&call(
        "get_document_outline",
        63,
        &json!({"path": fixture_path()}),
    ));
    let outline = client.receive();
    let outline_structured = &outline["result"]["structuredContent"];
    assert!(outline_structured["symbols"]
        .as_array()
        .is_some_and(|items| items.iter().any(|item| item["name"] == "sendMessage")));
    assert_eq!(outline_structured["truncated"], false);
    assert!(outline_structured["symbols"]
        .as_array()
        .and_then(|items| items.first())
        .is_some_and(|item| item.get("file").is_none()));

    client.send(&call(
        "find_callees",
        64,
        &json!({"path": fixture_path(), "symbol": "sendMessage"}),
    ));
    let callees = client.receive();
    let callees_structured = &callees["result"]["structuredContent"];
    assert!(callees_structured["callees"]
        .as_array()
        .is_some_and(|items| items.iter().any(|item| item["callee"] == "validateInput")));
    assert_indexed_callees(callees_structured);

    client.send(&call(
        "get_diagnostics",
        65,
        &json!({"path": diagnostics_fixture_path()}),
    ));
    let diagnostics = client.receive();
    assert!(diagnostics["result"]["structuredContent"]["diagnostics"]
        .as_array()
        .is_some_and(|items| !items.is_empty()));

    client.send(&call(
        "get_call_hierarchy",
        66,
        &json!({"path": fixture_path(), "symbol": "sendMessage", "depth": 2}),
    ));
    let hierarchy = client.receive();
    let structured = &hierarchy["result"]["structuredContent"];
    assert!(structured["nodes"]
        .as_array()
        .is_some_and(|items| items.iter().any(|item| item["symbol"] == "sendMessage")));
    assert!(structured["files"].is_array());
    assert!(structured["nodes"].as_array().is_some_and(|nodes| {
        nodes.iter().all(|node| {
            node["fileIdx"].as_u64().is_some_and(|file_idx| {
                file_idx
                    < structured["files"]
                        .as_array()
                        .map_or(0, |files| files.len() as u64)
            }) && node.get("id").is_none()
                && node.get("file").is_none()
        })
    }));
    assert!(structured["edges"].as_array().is_some_and(|edges| {
        edges.iter().all(|edge| {
            edge["fromIdx"].as_u64().is_some_and(|from_idx| {
                from_idx
                    < structured["nodes"]
                        .as_array()
                        .map_or(0, |nodes| nodes.len() as u64)
            }) && edge["toIdx"].as_u64().is_some_and(|to_idx| {
                to_idx
                    < structured["nodes"]
                        .as_array()
                        .map_or(0, |nodes| nodes.len() as u64)
            })
        })
    }));

    client.send(&call(
        "get_call_hierarchy",
        67,
        &json!({"path": fixture_path(), "symbol": "sendMessage", "depth": 2, "direction": "callees"}),
    ));
    let directed = client.receive();
    let directed_content = &directed["result"]["structuredContent"];
    assert!(directed_content["nodes"]
        .as_array()
        .is_some_and(|items| items.iter().any(|item| item["symbol"] == "sendMessage")));
    assert!(directed_content["edges"]
        .as_array()
        .is_some_and(|edges| edges.iter().all(|edge| edge["relation"] == "callee")));

    client.shutdown();
}

#[test]
fn resolves_relative_paths_against_configured_workspace_root() {
    let mut client = McpClientProcess::start();
    let _ = client.initialize();

    client.send(&call(
        "list_symbols",
        50,
        &json!({"path": "tests/fixtures/sample.tsx"}),
    ));
    let response = client.receive();
    assert!(response["result"]["structuredContent"]["symbols"]
        .as_array()
        .is_some_and(|symbols| { symbols.iter().any(|symbol| symbol["name"] == "sendMessage") }));

    client.shutdown();
}

#[test]
fn handles_valid_invalid_and_unsupported_requests() {
    let mut client = McpClientProcess::start();
    let _ = client.initialize();

    client.send(&call(
        "read_symbol",
        10,
        &json!({"path": fixture_path(), "symbol": "sendMessage"}),
    ));
    let valid = client.receive();
    assert_eq!(valid["id"], 10);
    assert!(valid["result"]["structuredContent"]["source"]
        .as_str()
        .is_some_and(|source| source.contains("function sendMessage")));

    client.send(&call(
        "read_symbol",
        11,
        &json!({"path": fixture_path(), "symbol": "missing"}),
    ));
    let invalid = client.receive();
    assert_eq!(invalid["id"], 11);
    assert_eq!(invalid["error"]["code"], -32602);

    client.send(&call(
        "list_symbols",
        12,
        &json!({"path": "unsupported.py"}),
    ));
    let unsupported = client.receive();
    assert_eq!(unsupported["id"], 12);
    assert_eq!(
        unsupported["result"]["structuredContent"],
        json!({"supported": false})
    );

    client.shutdown();
}

#[test]
fn handles_concurrent_requests() {
    let mut client = McpClientProcess::start();
    let _ = client.initialize();

    client.send(&call(
        "find_dependencies",
        20,
        &json!({"path": fixture_path(), "symbol": "sendMessage"}),
    ));
    client.send(&call(
        "read_symbol_context",
        21,
        &json!({"path": fixture_path(), "symbol": "sendMessage"}),
    ));

    let mut responses = HashMap::new();
    for _ in 0..2 {
        let response = client.receive();
        let id = response["id"].as_u64().expect("response should have an id");
        responses.insert(id, response);
    }
    assert_eq!(
        responses[&20]["result"]["structuredContent"]["dependencies"],
        json!(["Message", "validateInput", "sendMessage.normalize"])
    );
    assert_eq!(
        responses[&21]["result"]["structuredContent"]["requested_symbol"]["symbol"],
        "sendMessage"
    );

    client.shutdown();
}

#[test]
fn reports_session_statistics_only_for_successful_semantic_requests() {
    let mut client = McpClientProcess::start();
    let _ = client.initialize();

    client.send(&call(
        "read_symbol",
        30,
        &json!({"path": fixture_path(), "symbol": "sendMessage"}),
    ));
    let successful = client.receive();
    assert!(successful["result"]["structuredContent"]["source"].is_string());

    client.send(&call("list_symbols", 31, &json!({"path": fixture_path()})));
    let listed = client.receive();
    assert!(listed["result"]["structuredContent"]["symbols"].is_array());

    client.send(&call(
        "read_symbol",
        32,
        &json!({"path": fixture_path(), "symbol": "missing"}),
    ));
    let invalid = client.receive();
    assert_eq!(invalid["error"]["code"], -32602);

    client.send(&call("get_statistics", 33, &json!({})));
    let statistics = client.receive();
    assert_eq!(
        statistics["result"]["structuredContent"]["session"]["successful_requests"],
        2
    );
    assert_eq!(
        statistics["result"]["structuredContent"]["session"]["files_avoided"],
        2
    );
    assert_eq!(
        statistics["result"]["structuredContent"]["lifetime"]["files_avoided"],
        2
    );
    assert!(
        statistics["result"]["structuredContent"]["session"]["bytes_avoided"]
            .as_i64()
            .is_some_and(|value| value > 0)
    );
    assert!(
        statistics["result"]["structuredContent"]["session"]["estimated_token_savings"]
            .as_i64()
            .is_some_and(|value| value > 0)
    );
    assert!(statistics["result"]["structuredContent"]["session"]
        ["average_context_reduction_percent"]
        .as_f64()
        .is_some_and(|value| value > 0.0));

    client.shutdown();
}

#[test]
fn malformed_json_does_not_leave_a_server_process_running() {
    let mut client = McpClientProcess::start();
    let _ = client.initialize();
    client.send_raw("{malformed json");
    drop(client.stdin.take());
    let status = client
        .child
        .wait()
        .expect("server should exit after stdin closes");
    assert!(
        status.code().is_some(),
        "server should report a process exit"
    );
}
