use std::{
    fs,
    path::PathBuf,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn codescope_stats_prints_a_fresh_session_dashboard() {
    let output = Command::new(env!("CARGO_BIN_EXE_codescope"))
        .arg("stats")
        .output()
        .expect("codescope CLI should start");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("CLI output should be UTF-8");
    assert!(stdout.contains("CodeScope"));
    assert!(stdout.contains("Current Session"));
    assert!(stdout.contains("Lifetime"));
    assert!(stdout.contains("Estimated tokens (estimate)"));
    assert!(stdout.contains("Average reduction (estimate)"));
    assert!(stdout.contains("Files avoided:              0"));
}

#[test]
fn codescope_stats_reset_clears_only_lifetime_file() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after UNIX epoch")
        .as_nanos();
    let path: PathBuf = std::env::temp_dir().join(format!(
        "codescope-cli-stats-{}-{nonce}.json",
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

    let output = Command::new(env!("CARGO_BIN_EXE_codescope"))
        .env("CODESCOPE_STATS_PATH", &path)
        .args(["stats", "--reset"])
        .output()
        .expect("codescope CLI should start");

    assert!(output.status.success());
    let persisted = fs::read_to_string(&path).expect("reset statistics should remain readable");
    assert!(persisted.contains("\"filesAvoided\": 0"));
    assert!(persisted.contains("\"averageReduction\": 0.0"));
    let _ = fs::remove_file(path);
}
