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
