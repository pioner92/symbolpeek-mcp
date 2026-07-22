use std::{
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{SystemTime, UNIX_EPOCH},
};

use symbolpeek::{
    filesystem::SourceFile,
    language::{typescript::TypeScriptAdapter, LanguageAdapter},
    types::{DiagnosticsRequest, SearchSymbolsRequest},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn temp_project_dir() -> PathBuf {
    let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "symbolpeek-project-ts-{}-{sequence}-{stamp}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("create temp project");
    dir
}

fn source_file(path: PathBuf, source: &str) -> SourceFile {
    SourceFile {
        path,
        source: Arc::from(source.to_owned()),
        extension: "ts".to_owned(),
    }
}

fn diagnostics(file: &SourceFile) -> symbolpeek::types::DiagnosticsResult {
    TypeScriptAdapter
        .diagnostics(
            file,
            &DiagnosticsRequest {
                path: file.path.display().to_string(),
                symbol: None,
                max_results: None,
                offset: None,
            },
        )
        .expect("diagnostics should resolve")
}

/// The worker must prefer the project's own TypeScript over the bundled one.
///
/// The project here ships a non-functional `node_modules/typescript` shim. If
/// the worker loads it (the intended behavior), parsing fails; if it silently
/// fell back to the bundled TypeScript, parsing would succeed. Asserting the
/// failure proves the project's TypeScript takes precedence. The rest of the
/// suite — whose fixtures have no local TypeScript — exercises the fallback.
#[test]
fn prefers_the_projects_typescript_over_the_bundled_runtime() {
    let root = temp_project_dir();
    fs::write(root.join("package.json"), "{}").expect("write project marker");

    let ts_dir = root.join("node_modules/typescript");
    fs::create_dir_all(&ts_dir).expect("create shim package");
    fs::write(
        ts_dir.join("package.json"),
        r#"{"name":"typescript","version":"0.0.0-symbolpeek-shim","main":"index.js"}"#,
    )
    .expect("write shim manifest");
    // A TypeScript replacement missing every API the worker relies on.
    fs::write(
        ts_dir.join("index.js"),
        "module.exports = { __symbolpeekShim: true };\n",
    )
    .expect("write shim entry");

    let source = "export const value = 1;\n";
    let file_path = root.join("sample.ts");
    fs::write(&file_path, source).expect("write source");
    let file = SourceFile {
        path: file_path,
        source: Arc::from(source),
        extension: "ts".to_owned(),
    };

    let result = TypeScriptAdapter.parse(&file);
    assert!(
        result.is_err(),
        "worker should load the project's (shimmed) TypeScript and fail, not the bundled runtime"
    );

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn persistent_worker_reconciles_pinned_sources_across_file_switches() {
    let root = temp_project_dir();
    let src = root.join("src");
    fs::create_dir_all(&src).expect("create source directory");
    fs::write(
        root.join("tsconfig.json"),
        r#"{"compilerOptions":{"strict":true,"noEmit":true},"files":["src/a.ts","src/b.ts"]}"#,
    )
    .expect("write tsconfig");

    let a_path = src.join("a.ts");
    let b_path = src.join("b.ts");
    let invalid_a = "export const value: string = 1;\n";
    let valid_a = "export const value: string = \"ok\";\n";
    let valid_b = "export const other: number = 2;\n";
    fs::write(&a_path, invalid_a).expect("write a.ts");
    fs::write(&b_path, valid_b).expect("write b.ts");

    assert!(!diagnostics(&source_file(a_path.clone(), invalid_a))
        .diagnostics
        .is_empty());
    assert!(diagnostics(&source_file(b_path.clone(), valid_b))
        .diagnostics
        .is_empty());

    // Request text is authoritative while a file is pinned, even if it differs
    // from disk. Switching away must restore the disk snapshot without leaking
    // the request-only text into a later request.
    assert!(diagnostics(&source_file(a_path.clone(), valid_a))
        .diagnostics
        .is_empty());
    assert!(diagnostics(&source_file(b_path.clone(), valid_b))
        .diagnostics
        .is_empty());
    assert!(!diagnostics(&source_file(a_path.clone(), invalid_a))
        .diagnostics
        .is_empty());

    // A real disk update must invalidate the cached TypeScript snapshot.
    fs::write(&a_path, valid_a).expect("update a.ts");
    assert!(diagnostics(&source_file(b_path.clone(), valid_b))
        .diagnostics
        .is_empty());
    assert!(diagnostics(&source_file(a_path.clone(), valid_a))
        .diagnostics
        .is_empty());

    // Removing a previously pinned root must not poison the long-lived worker.
    fs::remove_file(&a_path).expect("remove a.ts");
    assert!(diagnostics(&source_file(b_path, valid_b))
        .diagnostics
        .is_empty());

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn workspace_search_reconciles_a_previously_pinned_request_source() {
    let root = temp_project_dir();
    let path = root.join("sample.ts");
    let disk_source = "export function DiskOnly() { return 1; }\n";
    let request_source = "export function RequestOnly() { return 2; }\n";
    fs::write(&path, disk_source).expect("write disk source");

    assert!(diagnostics(&source_file(path.clone(), request_source))
        .diagnostics
        .is_empty());
    let search = TypeScriptAdapter
        .search_symbols(&SearchSymbolsRequest {
            path: root.display().to_string(),
            query: String::new(),
            kind: None,
            max_results: Some(20),
            offset: None,
        })
        .expect("workspace search should reconcile the disk source");
    let names = search
        .symbols
        .iter()
        .map(|symbol| symbol.name.as_str())
        .collect::<Vec<_>>();
    assert!(names.contains(&"DiskOnly"), "got: {names:?}");
    assert!(!names.contains(&"RequestOnly"), "got: {names:?}");

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn files_outside_tsconfig_keep_the_import_closure_fallback() {
    let root = temp_project_dir();
    let src = root.join("src");
    let outside = root.join("outside");
    fs::create_dir_all(&src).expect("create source directory");
    fs::create_dir_all(&outside).expect("create outside directory");
    fs::write(
        root.join("tsconfig.json"),
        r#"{"compilerOptions":{"strict":true,"noEmit":true,"module":"NodeNext","moduleResolution":"NodeNext"},"files":["src/root.ts"]}"#,
    )
    .expect("write tsconfig");
    fs::write(src.join("root.ts"), "export const root = 1;\n").expect("write root.ts");
    fs::write(
        outside.join("shared.ts"),
        "export interface Shared { id: string }\n",
    )
    .expect("write shared.ts");
    let entry_source =
        "import type { Shared } from \"./shared\";\nexport const value: Shared = { id: \"ok\" };\n";
    let entry_path = outside.join("entry.ts");
    fs::write(&entry_path, entry_source).expect("write entry.ts");

    let result = diagnostics(&source_file(entry_path, entry_source));
    assert!(
        result.diagnostics.is_empty(),
        "an excluded entry file should still resolve its excluded dependency: {:?}",
        result.diagnostics
    );

    let _ = fs::remove_dir_all(&root);
}
