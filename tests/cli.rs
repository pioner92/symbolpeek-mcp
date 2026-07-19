use std::{
    fs,
    path::PathBuf,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn symbolpeek_stats_prints_lifetime_dashboard() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after UNIX epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "symbolpeek-cli-empty-stats-{}-{nonce}.json",
        std::process::id()
    ));
    let output = Command::new(env!("CARGO_BIN_EXE_symbolpeek"))
        .env("SYMBOLPEEK_STATS_PATH", &path)
        .arg("stats")
        .output()
        .expect("symbolpeek CLI should start");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("CLI output should be UTF-8");
    assert!(stdout.contains("SymbolPeek"));
    assert!(stdout.contains("SymbolPeek Lifetime Statistics"));
    assert!(!stdout.contains("Current Session"));
    assert!(stdout.contains("Estimated tokens:"));
    assert!(stdout.contains("Average reduction:"));
    assert!(stdout.contains("Efficiency meter:"));
    assert!(stdout.contains("Files avoided:"));
    let _ = fs::remove_file(path);
}

#[test]
fn symbolpeek_stats_reset_clears_only_lifetime_file() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after UNIX epoch")
        .as_nanos();
    let path: PathBuf = std::env::temp_dir().join(format!(
        "symbolpeek-cli-stats-{}-{nonce}.json",
        std::process::id()
    ));
    fs::write(
        &path,
        r#"{
  "filesAvoided": 8,
  "linesAvoided": 100,
  "bytesAvoided": 1000,
  "estimatedTokensAvoided": 250,
  "averageReduction": 91.4
}"#,
    )
    .expect("test statistics file should be writable");

    let output = Command::new(env!("CARGO_BIN_EXE_symbolpeek"))
        .env("SYMBOLPEEK_STATS_PATH", &path)
        .args(["stats", "--reset"])
        .output()
        .expect("symbolpeek CLI should start");

    assert!(output.status.success());
    let persisted = fs::read_to_string(&path).expect("reset statistics should remain readable");
    assert!(persisted.contains("\"filesAvoided\": 0"));
    assert!(persisted.contains("\"averageReduction\": 0.0"));
    let _ = fs::remove_file(path);
}

#[test]
fn sym_alias_prints_symbolpeek_help() {
    let output = Command::new(env!("CARGO_BIN_EXE_sym"))
        .arg("--help")
        .output()
        .expect("sym alias should start");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("CLI output should be UTF-8");
    assert!(stdout.contains("SymbolPeek"));
    assert!(stdout.contains("symbolpeek"));
    assert!(stdout.contains("sym"));
}
