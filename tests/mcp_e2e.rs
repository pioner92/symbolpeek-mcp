use std::{
    collections::{HashMap, HashSet},
    io::{BufRead, BufReader, Write},
    path::{Component, Path},
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
        Self::start_with_workspace_root(Some(Path::new(env!("CARGO_MANIFEST_DIR"))))
    }

    fn start_with_workspace_root(workspace_root: Option<&Path>) -> Self {
        let sequence = NEXT_STATISTICS_PATH.fetch_add(1, Ordering::Relaxed);
        let statistics_path = std::env::temp_dir().join(format!(
            "symbolpeek-mcp-e2e-{}-{sequence}.json",
            std::process::id()
        ));
        let mut command = Command::new(env!("CARGO_BIN_EXE_symbolpeek"));
        command
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .env("SYMBOLPEEK_STATS_PATH", &statistics_path)
            .env("SYMBOLPEEK_ALLOW_CWD_FALLBACK", "false")
            .env_remove("SYMBOLPEEK_WORKSPACE_ROOT")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        if let Some(workspace_root) = workspace_root {
            command.env("SYMBOLPEEK_WORKSPACE_ROOT", workspace_root);
        }
        let mut child = command.spawn().expect("MCP server should start");
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
        self.initialize_with_capabilities(&json!({}))
    }

    fn initialize_with_capabilities(&mut self, capabilities: &Value) -> Value {
        self.send(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": capabilities,
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

fn assert_tool_argument_error(response: &Value, expected: &str) {
    assert_eq!(response["result"]["isError"], true);
    assert!(response["result"]["content"]
        .as_array()
        .and_then(|content| content.first())
        .and_then(|content| content["text"].as_str())
        .is_some_and(|message| {
            message.contains("failed to deserialize parameters") && message.contains(expected)
        }));
}

fn assert_compact_indexed_rows(result: &Value, key: &str, expected_fields: &[&str]) {
    let files = result["files"]
        .as_array()
        .expect("compact response should include a files table");
    if let Some(base) = result.get("base").and_then(Value::as_str) {
        assert!(Path::new(base).is_absolute());
        assert!(files.iter().all(|file| {
            file.as_str().is_some_and(|file| {
                let file = Path::new(file);
                !file.is_absolute()
                    && !file
                        .components()
                        .any(|component| component == Component::ParentDir)
                    && Path::new(base).join(file).is_absolute()
            })
        }));
    } else {
        assert!(files.iter().all(|file| file
            .as_str()
            .is_some_and(|file| Path::new(file).is_absolute())));
    }
    assert_eq!(result["fields"], json!(expected_fields));
    assert!(result["truncated"].is_boolean());
    assert!(result[key].as_array().is_some_and(|items| {
        items.iter().all(|item| {
            item.as_array().is_some_and(|row| {
                row.len() == expected_fields.len()
                    && row[0]
                        .as_u64()
                        .is_some_and(|index| index < u64::try_from(files.len()).unwrap_or(u64::MAX))
            })
        })
    }));
}

fn assert_ts_semantic_analysis(content: &Value) {
    assert_eq!(content["analysis"]["backend"], "ts-compiler-api");
    assert_eq!(content["analysis"]["analysis_level"], "semantic");
    assert_eq!(content["analysis"]["complete"], true);
}

fn assert_outline_rows(rows: &Value) {
    let rows = rows
        .as_array()
        .expect("compact outline symbols should be tuple rows");
    for row in rows {
        let row = row
            .as_array()
            .expect("each compact outline symbol should be a tuple row");
        assert_eq!(row.len(), 5);
        assert!(row[0].is_string());
        assert!(row[1].is_string());
        assert!(row[2].is_u64());
        assert!(row[3].is_u64());
        assert_outline_rows(&row[4]);
    }
}

fn outline_contains(rows: &Value, name: &str) -> bool {
    rows.as_array().is_some_and(|rows| {
        rows.iter().any(|row| {
            row.as_array().is_some_and(|row| {
                row.first().and_then(Value::as_str) == Some(name)
                    || row
                        .get(4)
                        .is_some_and(|children| outline_contains(children, name))
            })
        })
    })
}

fn assert_indexed_callees(result: &Value) {
    let files = result["files"]
        .as_array()
        .expect("compact callees should include a files table");
    assert_eq!(
        result["fields"],
        json!([
            "callee",
            "file_idx",
            "start_line",
            "end_line",
            "start_column",
            "end_column",
            "definition"
        ])
    );
    assert_eq!(
        result["definition_fields"],
        json!([
            "file_idx",
            "start_line",
            "end_line",
            "start_column",
            "end_column"
        ])
    );
    assert!(result["callees"].as_array().is_some_and(|items| {
        items.iter().all(|item| {
            item.as_array().is_some_and(|row| {
                row.len() == 7
                    && row[1]
                        .as_u64()
                        .is_some_and(|index| index < files.len() as u64)
                    && (row[6].is_null()
                        || row[6].as_array().is_some_and(|definition| {
                            definition.len() == 5
                                && definition[0]
                                    .as_u64()
                                    .is_some_and(|index| index < files.len() as u64)
                        }))
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

fn callee_edge_fixture_path() -> String {
    format!(
        "{}/tests/fixtures/navigation/callee_edge_cases.ts",
        env!("CARGO_MANIFEST_DIR")
    )
}

fn diagnostics_fixture_path() -> String {
    format!(
        "{}/tests/fixtures/navigation/diagnostics.ts",
        env!("CARGO_MANIFEST_DIR")
    )
}

fn screens_fixture_path() -> String {
    format!(
        "{}/tests/fixtures/navigation/screens.ts",
        env!("CARGO_MANIFEST_DIR")
    )
}

fn screen_usage_fixture_path() -> String {
    format!(
        "{}/tests/fixtures/navigation/screen_usage.ts",
        env!("CARGO_MANIFEST_DIR")
    )
}

fn rust_fixture_path() -> String {
    format!(
        "{}/tests/fixtures/rust/sample.rs",
        env!("CARGO_MANIFEST_DIR")
    )
}

fn assert_exact_path_schema_descriptions(tools: &[Value]) {
    for name in [
        "read_symbol",
        "list_symbols",
        "find_dependencies",
        "find_references",
        "find_callers",
        "go_to_definition",
        "read_symbol_context",
        "get_type",
        "find_implementations",
        "get_document_outline",
        "find_callees",
        "get_diagnostics",
        "get_call_hierarchy",
    ] {
        let path_description = tools
            .iter()
            .find(|tool| tool["name"] == name)
            .and_then(|tool| tool.pointer("/inputSchema/properties/path/description"))
            .and_then(Value::as_str)
            .expect("file-based tool should describe its path input");
        assert!(path_description.starts_with("Exact source file"));
        assert!(path_description.contains("MCP-root"));
        assert!(path_description.contains("no module/dir/index lookup"));
    }
    let search_path_description = tools
        .iter()
        .find(|tool| tool["name"] == "search_symbols")
        .and_then(|tool| tool.pointer("/inputSchema/properties/path/description"))
        .and_then(Value::as_str)
        .expect("search_symbols should describe its workspace path input");
    assert!(search_path_description.starts_with("Workspace dir"));
    assert!(search_path_description.contains("MCP-root"));
    assert!(!search_path_description.contains("index resolution"));
}

fn assert_language_support_markers(tools: &[Value]) {
    for name in [
        "read_symbol",
        "list_symbols",
        "search_symbols",
        "get_document_outline",
        "find_dependencies",
        "read_symbol_context",
    ] {
        let description = tools
            .iter()
            .find(|tool| tool["name"] == name)
            .and_then(|tool| tool["description"].as_str())
            .expect("syntax tool should publish a description");
        assert!(description.starts_with("[.ts/.tsx/.js/.jsx/.rs/.py/.java/.go]"));
    }
    let implementations = tools
        .iter()
        .find(|tool| tool["name"] == "find_implementations")
        .and_then(|tool| tool["description"].as_str())
        .expect("implementation tool should publish a description");
    assert!(implementations.starts_with("[.ts/.tsx/.js/.jsx/.rs]"));
    for name in [
        "find_references",
        "find_callers",
        "go_to_definition",
        "get_type",
        "find_callees",
        "get_diagnostics",
        "get_call_hierarchy",
    ] {
        let description = tools
            .iter()
            .find(|tool| tool["name"] == name)
            .and_then(|tool| tool["description"].as_str())
            .expect("semantic tool should publish a description");
        assert!(description.starts_with("[.ts/.tsx/.js/.jsx]"));
    }
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
    assert!(names.contains(&"get_capabilities"));
    assert!(names.contains(&"get_statistics"));

    assert_language_support_markers(tools);
    assert_exact_path_schema_descriptions(tools);

    for name in [
        "search_symbols",
        "find_references",
        "find_callers",
        "find_callees",
        "find_implementations",
    ] {
        let description = tools
            .iter()
            .find(|tool| tool["name"] == name)
            .and_then(|tool| tool["description"].as_str())
            .expect("paginated cross-file tool should publish its description");
        assert!(description.contains("file_idx are page-local"));
    }
    let list_description = tools
        .iter()
        .find(|tool| tool["name"] == "list_symbols")
        .and_then(|tool| tool["description"].as_str())
        .expect("list_symbols should publish its description");
    assert!(list_description.contains("one file/page"));

    let callee_description = tools
        .iter()
        .find(|tool| tool["name"] == "find_callees")
        .and_then(|tool| tool["description"].as_str())
        .expect("find_callees should publish its description");
    assert!(callee_description.contains("rows=fields"));
    assert!(callee_description.contains("definition_fields"));
    assert!(callee_description.contains("definition:null"));
    assert!(callee_description.contains("dynamic anonymous"));

    let hierarchy_description = tools
        .iter()
        .find(|tool| tool["name"] == "get_call_hierarchy")
        .and_then(|tool| tool["description"].as_str())
        .expect("get_call_hierarchy should publish its description");
    assert!(hierarchy_description.contains("node_fields/edge_fields"));
    assert!(hierarchy_description.contains("[caller_idx,callee_idx]"));

    let outline_description = tools
        .iter()
        .find(|tool| tool["name"] == "get_document_outline")
        .and_then(|tool| tool["description"].as_str())
        .expect("get_document_outline should publish its description");
    assert!(outline_description.contains("recursive rows follow fields"));
    assert!(outline_description.contains("every level"));

    client.send(&call("get_statistics", 3, &json!({})));
    let empty_statistics = client.receive();
    assert_eq!(
        empty_statistics["result"]["structuredContent"]["session"]["successful_requests"],
        0
    );
    assert_eq!(
        empty_statistics["result"]["structuredContent"]["session"]["files_avoided"],
        0
    );

    client.send(&call("get_capabilities", 4, &json!({})));
    let capability_response = client.receive();
    let capabilities = &capability_response["result"]["structuredContent"];
    assert_eq!(
        capabilities["language_fields"],
        json!(["extensions", "backend", "levels"])
    );
    assert!(capabilities["languages"]["rust"].is_array());
    assert_eq!(capabilities["languages"]["rust"][0], json!([".rs"]));
    assert_eq!(capabilities["languages"]["rust"][1], "tree-sitter");

    client.shutdown();
}

#[test]
#[allow(clippy::too_many_lines)]
fn serves_reliable_rust_syntax_tools_and_metadata() {
    let mut client = McpClientProcess::start();
    let _ = client.initialize();
    let path = rust_fixture_path();

    client.send(&call(
        "read_symbol",
        200,
        &json!({"path": path, "symbol": "Client.send"}),
    ));
    let read = client.receive();
    let read = &read["result"]["structuredContent"];
    assert_eq!(read["kind"], "method");
    assert_eq!(read["analysis"]["backend"], "tree-sitter");
    assert_eq!(read["analysis"]["analysis_level"], "syntax");
    assert_eq!(read["analysis"]["complete"], true);

    client.send(&call(
        "list_symbols",
        201,
        &json!({"path": rust_fixture_path()}),
    ));
    let list = client.receive();
    let list = &list["result"]["structuredContent"];
    assert!(list["symbols"].as_array().is_some_and(|symbols| {
        symbols
            .iter()
            .any(|symbol| symbol[0] == "Client" && symbol[1] == "struct")
    }));
    assert_eq!(list["analysis"]["backend"], "tree-sitter");

    client.send(&call(
        "get_document_outline",
        202,
        &json!({"path": rust_fixture_path()}),
    ));
    let outline = client.receive();
    let outline = &outline["result"]["structuredContent"];
    assert!(outline_contains(&outline["symbols"], "send"));
    assert_eq!(outline["analysis"]["analysis_level"], "syntax");

    client.send(&call(
        "search_symbols",
        203,
        &json!({
            "path": format!("{}/tests/fixtures/rust", env!("CARGO_MANIFEST_DIR")),
            "query": "send",
            "kind": "method"
        }),
    ));
    let search = client.receive();
    let search = &search["result"]["structuredContent"];
    assert!(search["symbols"]
        .as_array()
        .is_some_and(|symbols| symbols.len() == 3));
    assert_eq!(search["analysis"]["backend"], "tree-sitter");

    client.send(&call(
        "find_dependencies",
        206,
        &json!({"path": rust_fixture_path(), "symbol": "bounded_size"}),
    ));
    let dependencies = client.receive();
    let dependencies = &dependencies["result"]["structuredContent"];
    assert_eq!(
        dependencies["dependencies"],
        json!(["DEFAULT_LIMIT", "normalized_size"])
    );
    assert_eq!(dependencies["analysis"]["backend"], "tree-sitter");

    client.send(&call(
        "read_symbol_context",
        207,
        &json!({"path": rust_fixture_path(), "symbol": "bounded_size"}),
    ));
    let context = client.receive();
    let context = &context["result"]["structuredContent"];
    assert_eq!(context["helper_functions"][0]["symbol"], "normalized_size");
    assert_eq!(context["local_constants"][0]["symbol"], "DEFAULT_LIMIT");

    client.send(&call(
        "find_implementations",
        208,
        &json!({"path": rust_fixture_path(), "symbol": "Transport"}),
    ));
    let implementations = client.receive();
    let implementations = &implementations["result"]["structuredContent"];
    assert_eq!(implementations["analysis"]["backend"], "tree-sitter");
    assert!(implementations["impls"].as_array().is_some_and(|impls| {
        impls
            .iter()
            .any(|implementation| implementation[1] == "impl Transport for Client")
    }));

    client.send(&call(
        "find_references",
        204,
        &json!({"path": rust_fixture_path(), "symbol": "Client"}),
    ));
    let unsupported = client.receive();
    assert_eq!(unsupported["error"]["code"], -32602);
    assert!(unsupported["error"]["message"]
        .as_str()
        .is_some_and(|message| message.contains("not supported")));

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
    assert_ts_semantic_analysis(references_structured);
    assert!(references_structured["refs"]
        .as_array()
        .is_some_and(|items| items.len() >= 3));
    assert_compact_indexed_rows(
        references_structured,
        "refs",
        &[
            "file_idx",
            "start_line",
            "end_line",
            "start_column",
            "end_column",
            "is_definition",
        ],
    );

    client.send(&call(
        "find_references",
        43,
        &json!({"path": path, "symbol": "useAuth", "max_results": 1}),
    ));
    let limited_references = client.receive();
    assert_eq!(
        limited_references["result"]["structuredContent"]["refs"]
            .as_array()
            .map_or(0, Vec::len),
        1
    );
    assert_eq!(
        limited_references["result"]["structuredContent"]["truncated"],
        true
    );
    assert_eq!(
        limited_references["result"]["structuredContent"]["next_offset"],
        1
    );

    client.send(&call(
        "find_references",
        44,
        &json!({"path": navigation_fixture_path(), "symbol": "useAuth", "max_results": 1, "offset": 1}),
    ));
    let second_reference_page = client.receive();
    assert_eq!(
        second_reference_page["result"]["structuredContent"]["refs"]
            .as_array()
            .map_or(0, Vec::len),
        1
    );

    client.send(&call(
        "find_callers",
        41,
        &json!({"path": navigation_fixture_path(), "symbol": "useAuth"}),
    ));
    let callers = client.receive();
    let callers_structured = &callers["result"]["structuredContent"];
    assert_ts_semantic_analysis(callers_structured);
    assert!(callers_structured["callers"]
        .as_array()
        .is_some_and(|items| { items.iter().any(|item| item[1] == "Dashboard") }));
    assert_compact_indexed_rows(
        callers_structured,
        "callers",
        &[
            "file_idx",
            "caller",
            "start_line",
            "end_line",
            "start_column",
            "end_column",
        ],
    );

    client.send(&call(
        "go_to_definition",
        42,
        &json!({"path": navigation_fixture_path(), "line": 5, "column": 20}),
    ));
    let definition = client.receive();
    assert_ts_semantic_analysis(&definition["result"]["structuredContent"]);
    assert!(
        definition["result"]["structuredContent"]["definition"]["file"]
            .as_str()
            .is_some_and(|file| file.ends_with("navigation/auth.ts"))
    );

    client.shutdown();
}

#[test]
fn handles_qualified_enum_members() {
    let mut client = McpClientProcess::start();
    let _ = client.initialize();
    let symbol = "Screens.PUBLISH_ACKNOWLEDGEMENT";

    client.send(&call(
        "read_symbol",
        90,
        &json!({"path": screens_fixture_path(), "symbol": symbol}),
    ));
    let read = client.receive();
    let read_structured = &read["result"]["structuredContent"];
    assert_eq!(read_structured["symbol"], symbol);
    assert_eq!(read_structured["kind"], "enum_member");
    assert_eq!(
        read_structured["source"],
        "PUBLISH_ACKNOWLEDGEMENT = \"publishAcknowledgement\""
    );

    client.send(&call(
        "find_references",
        91,
        &json!({"path": screen_usage_fixture_path(), "symbol": symbol}),
    ));
    let references = client.receive();
    let references_structured = &references["result"]["structuredContent"];
    assert_compact_indexed_rows(
        references_structured,
        "refs",
        &[
            "file_idx",
            "start_line",
            "end_line",
            "start_column",
            "end_column",
            "is_definition",
        ],
    );
    assert!(references_structured["refs"]
        .as_array()
        .is_some_and(|items| items.len() >= 3));

    client.send(&call(
        "read_symbol",
        92,
        &json!({"path": screens_fixture_path(), "symbol": "Screens.DOES_NOT_EXIST"}),
    ));
    let missing_member = client.receive();
    assert_eq!(missing_member["error"]["code"], -32602);
    assert!(missing_member["error"]["message"]
        .as_str()
        .is_some_and(|message| message.contains("parent exists")));

    client.send(&call(
        "read_symbol",
        93,
        &json!({"path": screens_fixture_path(), "symbol": "Missing.DOES_NOT_EXIST"}),
    ));
    let missing_parent = client.receive();
    assert_eq!(missing_parent["error"]["code"], -32602);
    assert!(missing_parent["error"]["message"]
        .as_str()
        .is_some_and(|message| !message.contains("parent exists")));

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
        &json!({"path": navigation_workspace_path(), "query": "", "max_results": 1}),
    ));
    let search = client.receive();
    let search_structured = &search["result"]["structuredContent"];
    assert_eq!(
        search_structured["symbols"].as_array().map(Vec::len),
        Some(1)
    );
    assert_eq!(search_structured["truncated"], true);
    assert_eq!(search_structured["next_offset"], 1);
    assert_compact_indexed_rows(
        search_structured,
        "symbols",
        &[
            "file_idx",
            "name",
            "kind",
            "start_line",
            "end_line",
            "start_column",
            "end_column",
        ],
    );
    let first_symbol = search_structured["symbols"][0][1].clone();

    client.send(&call(
        "search_symbols",
        600,
        &json!({"path": navigation_workspace_path(), "query": "", "max_results": 1, "offset": 1}),
    ));
    let next_search = client.receive();
    let next_search_structured = &next_search["result"]["structuredContent"];
    assert_compact_indexed_rows(
        next_search_structured,
        "symbols",
        &[
            "file_idx",
            "name",
            "kind",
            "start_line",
            "end_line",
            "start_column",
            "end_column",
        ],
    );
    assert_eq!(
        next_search_structured["symbols"].as_array().map(Vec::len),
        Some(1)
    );
    assert_ne!(next_search_structured["symbols"][0][1], first_symbol);
    assert_eq!(next_search_structured["next_offset"], 2);

    client.send(&call(
        "get_type",
        61,
        &json!({"path": navigation_fixture_path(), "line": 5, "column": 20}),
    ));
    let type_info = client.receive();
    assert_ts_semantic_analysis(&type_info["result"]["structuredContent"]);
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
    assert_ts_semantic_analysis(implementations_structured);
    assert!(implementations_structured["impls"]
        .as_array()
        .is_some_and(|items| items.len() >= 2));
    assert!(implementations_structured["impls"]
        .as_array()
        .is_some_and(|items| {
            items.iter().any(|item| item[1] == "MemoryRepository")
                && items.iter().any(|item| item[1] == "CachedRepository")
        }));
    assert_compact_indexed_rows(
        implementations_structured,
        "impls",
        &[
            "file_idx",
            "symbol",
            "start_line",
            "end_line",
            "start_column",
            "end_column",
        ],
    );

    client.send(&call(
        "get_document_outline",
        63,
        &json!({"path": fixture_path()}),
    ));
    let outline = client.receive();
    let outline_structured = &outline["result"]["structuredContent"];
    assert_eq!(
        outline_structured["fields"],
        json!(["name", "kind", "start_line", "end_line", "children"])
    );
    assert_outline_rows(&outline_structured["symbols"]);
    assert!(outline_contains(
        &outline_structured["symbols"],
        "sendMessage"
    ));
    assert!(outline_contains(
        &outline_structured["symbols"],
        "normalize"
    ));
    assert_eq!(outline_structured["truncated"], false);
    assert!(outline_structured.get("supported").is_none());

    client.send(&call(
        "find_callees",
        64,
        &json!({"path": fixture_path(), "symbol": "sendMessage"}),
    ));
    let callees = client.receive();
    let callees_structured = &callees["result"]["structuredContent"];
    assert_ts_semantic_analysis(callees_structured);
    assert!(callees_structured["callees"]
        .as_array()
        .is_some_and(|items| items.iter().any(|item| item[0] == "validateInput")));
    assert_indexed_callees(callees_structured);

    client.send(&call(
        "get_diagnostics",
        65,
        &json!({"path": diagnostics_fixture_path()}),
    ));
    let diagnostics = client.receive();
    let diagnostics_structured = &diagnostics["result"]["structuredContent"];
    assert_ts_semantic_analysis(diagnostics_structured);
    assert!(diagnostics_structured["diagnostics"]
        .as_array()
        .is_some_and(|items| !items.is_empty()));
    assert_eq!(diagnostics_structured["truncated"], false);
    assert!(diagnostics_structured["diagnostics"]
        .as_array()
        .and_then(|items| items.first())
        .is_some_and(|item| item.get("file").is_none()));

    client.send(&call(
        "get_call_hierarchy",
        66,
        &json!({"path": fixture_path(), "symbol": "sendMessage", "depth": 2}),
    ));
    let hierarchy = client.receive();
    let structured = &hierarchy["result"]["structuredContent"];
    assert_ts_semantic_analysis(structured);
    assert_eq!(
        structured["node_fields"],
        json!([
            "symbol",
            "file_idx",
            "start_line",
            "end_line",
            "hub",
            "callers_elided"
        ])
    );
    assert_eq!(
        structured["edge_fields"],
        json!(["caller_idx", "callee_idx"])
    );
    assert!(structured["nodes"]
        .as_array()
        .is_some_and(|items| items.iter().any(|item| item[0] == "sendMessage")));
    assert!(structured["files"].is_array());
    assert!(structured["nodes"].as_array().is_some_and(|nodes| {
        nodes.iter().all(|node| {
            node.as_array().is_some_and(|node| {
                node.len() == 6
                    && node[1].as_u64().is_some_and(|file_idx| {
                        file_idx
                            < structured["files"]
                                .as_array()
                                .map_or(0, |files| files.len() as u64)
                    })
            })
        })
    }));
    assert!(structured["edges"].as_array().is_some_and(|edges| {
        edges.iter().all(|edge| {
            edge.as_array().is_some_and(|edge| {
                edge.len() == 2
                    && edge[0].as_u64().is_some_and(|caller_idx| {
                        caller_idx
                            < structured["nodes"]
                                .as_array()
                                .map_or(0, |nodes| nodes.len() as u64)
                    })
                    && edge[1].as_u64().is_some_and(|callee_idx| {
                        callee_idx
                            < structured["nodes"]
                                .as_array()
                                .map_or(0, |nodes| nodes.len() as u64)
                    })
            })
        })
    }));
    let unique_edges = structured["edges"]
        .as_array()
        .expect("hierarchy edges should be an array")
        .iter()
        .map(|edge| (edge[0].as_u64(), edge[1].as_u64()))
        .collect::<HashSet<_>>();
    assert_eq!(
        unique_edges.len(),
        structured["edges"].as_array().map_or(0, Vec::len)
    );

    client.send(&call(
        "get_call_hierarchy",
        67,
        &json!({"path": fixture_path(), "symbol": "sendMessage", "depth": 2, "direction": "callees"}),
    ));
    let directed = client.receive();
    let directed_content = &directed["result"]["structuredContent"];
    assert!(directed_content["nodes"]
        .as_array()
        .is_some_and(|items| items.iter().any(|item| item[0] == "sendMessage")));
    let directed_nodes = directed_content["nodes"]
        .as_array()
        .expect("directed hierarchy nodes should be an array");
    let root_index = directed_nodes
        .iter()
        .position(|node| node[0] == "sendMessage")
        .expect("directed hierarchy should contain its root");
    let target_index = directed_nodes
        .iter()
        .position(|node| node[0] == "validateInput")
        .expect("directed hierarchy should contain the expected callee");
    assert!(directed_content["edges"]
        .as_array()
        .is_some_and(|edges| edges.contains(&json!([root_index, target_index]))));

    client.send(&call(
        "find_callees",
        68,
        &json!({"path": callee_edge_fixture_path(), "symbol": "inspectCalls"}),
    ));
    let edge_callees = client.receive();
    let edge_content = &edge_callees["result"]["structuredContent"];
    assert_indexed_callees(edge_content);
    let rows = edge_content["callees"]
        .as_array()
        .expect("edge-case callees should be tuple rows");
    assert!(rows
        .iter()
        .any(|row| row[0] == "canonicalTarget" && row[6].is_array()));
    assert!(rows
        .iter()
        .any(|row| row[0] == "MissingConstructor" && row[6].is_null()));
    assert!(edge_content["base"]
        .as_str()
        .is_some_and(|base| Path::new(base).is_absolute()));
    assert!(edge_content["files"].as_array().is_some_and(|files| {
        files.iter().all(|file| {
            file.as_str()
                .is_some_and(|file| !Path::new(file).is_absolute())
        })
    }));

    client.send(&call(
        "find_callees",
        69,
        &json!({"path": contracts_fixture_path(), "symbol": "MemoryRepository.load"}),
    ));
    let empty_callees = client.receive();
    let empty_content = &empty_callees["result"]["structuredContent"];
    assert_indexed_callees(empty_content);
    assert_eq!(empty_content["callees"], json!([]));
    assert_eq!(empty_content["files"], json!([]));
    assert!(empty_content.get("base").is_none());
    assert_eq!(empty_content["truncated"], false);

    client.shutdown();
}

#[test]
fn resolves_relative_paths_against_configured_workspace_root() {
    let mut client = McpClientProcess::start();
    let _ = client.initialize_with_capabilities(&json!({"roots": {}}));

    client.send(&call(
        "list_symbols",
        50,
        &json!({"path": "tests/fixtures/sample.tsx"}),
    ));
    let response = client.receive();
    let structured = &response["result"]["structuredContent"];
    assert!(structured["symbols"]
        .as_array()
        .is_some_and(|symbols| { symbols.iter().any(|symbol| symbol[0] == "sendMessage") }));
    assert_eq!(
        structured["fields"],
        json!(["name", "kind", "start_line", "end_line", "module_specifier"])
    );
    assert_eq!(structured["truncated"], false);
    assert!(structured["symbols"]
        .as_array()
        .and_then(|symbols| symbols.first())
        .and_then(Value::as_array)
        .is_some_and(|symbol| symbol.len() == 5));

    client.shutdown();
}

#[test]
fn resolves_first_relative_request_against_mcp_client_roots() {
    let workspace = std::env::temp_dir().join(format!(
        "symbolpeek roots {}",
        NEXT_STATISTICS_PATH.fetch_add(1, Ordering::Relaxed)
    ));
    let source_path = workspace.join("src/rooted.ts");
    std::fs::create_dir_all(source_path.parent().expect("source should have parent"))
        .expect("workspace should be creatable");
    std::fs::write(&source_path, "export const fromClientRoot = 1;\n")
        .expect("rooted fixture should be writable");

    let mut client = McpClientProcess::start_with_workspace_root(None);
    let _ = client.initialize_with_capabilities(&json!({"roots": {"listChanged": true}}));
    client.send(&call("list_symbols", 70, &json!({"path": "src/rooted.ts"})));

    let roots_request = client.receive();
    assert_eq!(roots_request["method"], "roots/list");
    let root_uri = format!("file://{}", workspace.to_string_lossy().replace(' ', "%20"));
    client.send(&json!({
        "jsonrpc": "2.0",
        "id": roots_request["id"].clone(),
        "result": {"roots": [{"uri": root_uri, "name": "fixture"}]}
    }));

    let response = client.receive();
    assert_eq!(response["id"], 70);
    assert!(response["result"]["structuredContent"]["symbols"]
        .as_array()
        .is_some_and(|symbols| symbols.iter().any(|symbol| symbol[0] == "fromClientRoot")));

    client.shutdown();
    std::fs::remove_dir_all(workspace).expect("workspace should be removable");
}

#[test]
fn rejects_unbased_relative_paths_but_keeps_absolute_paths_working() {
    let mut client = McpClientProcess::start_with_workspace_root(None);
    let _ = client.initialize();

    client.send(&call(
        "list_symbols",
        71,
        &json!({"path": "tests/fixtures/sample.tsx"}),
    ));
    let relative = client.receive();
    assert_eq!(relative["error"]["code"], -32602);
    assert!(relative["error"]["message"]
        .as_str()
        .is_some_and(|message| message.contains("no workspace root is available")));
    assert!(!relative["error"]["message"]
        .as_str()
        .is_some_and(|message| message.contains(env!("CARGO_MANIFEST_DIR"))));

    client.send(&call("list_symbols", 72, &json!({"path": fixture_path()})));
    let absolute = client.receive();
    assert!(absolute["result"]["structuredContent"]["symbols"]
        .as_array()
        .is_some_and(|symbols| symbols.iter().any(|symbol| symbol[0] == "sendMessage")));

    client.shutdown();
}

#[test]
fn multi_root_paths_require_exactly_one_existing_match() {
    let sequence = NEXT_STATISTICS_PATH.fetch_add(1, Ordering::Relaxed);
    let root_a = std::env::temp_dir().join(format!("symbolpeek-multi-a-{sequence}"));
    let root_b = std::env::temp_dir().join(format!("symbolpeek-multi-b-{sequence}"));
    for root in [&root_a, &root_b] {
        std::fs::create_dir_all(root.join("src")).expect("workspace should be creatable");
        std::fs::write(root.join("src/shared.ts"), "export const shared = 1;\n")
            .expect("shared fixture should be writable");
    }
    std::fs::write(
        root_a.join("src/unique.ts"),
        "export const uniqueToRootA = 1;\n",
    )
    .expect("unique fixture should be writable");

    let mut client = McpClientProcess::start_with_workspace_root(None);
    let _ = client.initialize_with_capabilities(&json!({"roots": {}}));
    client.send(&call("list_symbols", 73, &json!({"path": "src/unique.ts"})));
    let roots_request = client.receive();
    assert_eq!(roots_request["method"], "roots/list");
    client.send(&json!({
        "jsonrpc": "2.0",
        "id": roots_request["id"].clone(),
        "result": {"roots": [
            {"uri": format!("file://{}", root_a.display())},
            {"uri": format!("file://{}", root_b.display())}
        ]}
    }));
    let unique = client.receive();
    assert!(unique["result"]["structuredContent"]["symbols"]
        .as_array()
        .is_some_and(|symbols| symbols.iter().any(|symbol| symbol[0] == "uniqueToRootA")));

    client.send(&call("list_symbols", 74, &json!({"path": "src/shared.ts"})));
    let ambiguous = client.receive();
    assert_eq!(ambiguous["error"]["code"], -32602);
    assert!(ambiguous["error"]["message"]
        .as_str()
        .is_some_and(|message| message.contains("ambiguous across workspace roots")));
    assert!(ambiguous["error"]["message"]
        .as_str()
        .is_some_and(|message| message.contains(&root_a.display().to_string())));
    assert!(ambiguous["error"]["message"]
        .as_str()
        .is_some_and(|message| message.contains(&root_b.display().to_string())));

    client.shutdown();
    std::fs::remove_dir_all(root_a).expect("first workspace should be removable");
    std::fs::remove_dir_all(root_b).expect("second workspace should be removable");
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
        &json!({"path": "unsupported.kt"}),
    ));
    let unsupported = client.receive();
    assert_eq!(unsupported["id"], 12);
    assert_eq!(
        unsupported["result"]["structuredContent"],
        json!({"supported": false})
    );

    client.send(&call(
        "list_symbols",
        13,
        &json!({"path": fixture_path(), "max_results": 0}),
    ));
    let clamped = client.receive();
    assert_eq!(
        clamped["result"]["structuredContent"]["symbols"]
            .as_array()
            .map(Vec::len),
        Some(1)
    );
    assert_eq!(clamped["result"]["structuredContent"]["truncated"], true);

    client.send(&call(
        "find_references",
        14,
        &json!({"path": fixture_path(), "symbol": "sendMessage", "offset": -1}),
    ));
    let negative_offset = client.receive();
    assert_tool_argument_error(&negative_offset, "expected usize");

    client.send(&call(
        "get_call_hierarchy",
        15,
        &json!({"path": fixture_path(), "symbol": "sendMessage", "direction": "sideways"}),
    ));
    let invalid_direction = client.receive();
    assert_tool_argument_error(&invalid_direction, "unknown variant");

    client.send(&call("read_symbol", 16, &json!({"path": fixture_path()})));
    let missing_argument = client.receive();
    assert_tool_argument_error(&missing_argument, "missing field");

    client.send(&call(
        "read_symbol",
        17,
        &json!({"path": "tests/fixtures/missing.ts", "symbol": "anything"}),
    ));
    let missing_file = client.receive();
    assert_eq!(missing_file["error"]["code"], -32602);
    assert!(missing_file["error"]["message"]
        .as_str()
        .is_some_and(|message| message.contains("file not found")));

    client.send(&call(
        "get_diagnostics",
        18,
        &json!({"path": diagnostics_fixture_path(), "symbol": "definitelyMissing"}),
    ));
    let missing_scope = client.receive();
    assert_eq!(missing_scope["error"]["code"], -32602);
    assert!(missing_scope["error"]["message"]
        .as_str()
        .is_some_and(|message| message.contains("symbol 'definitelyMissing' was not found")));

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
    assert!(
        responses[&21]["result"]["structuredContent"]["requested_symbol"]
            .get("file")
            .is_none()
    );
    assert!(
        responses[&21]["result"]["structuredContent"]["requested_symbol"]
            .get("supported")
            .is_none()
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
