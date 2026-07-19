use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use codescope::{
    errors::CodeScopeError,
    filesystem::{is_supported, load_source},
};

fn test_directory() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "codescope-filesystem-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&path).expect("test directory should be creatable");
    path
}

#[test]
fn loads_current_source_bytes_from_unicode_filename() {
    let directory = test_directory();
    let path = directory.join("προσθήκη.ts");
    let source = "// preserve this\nexport const value = '😀';\n";
    fs::write(&path, source.as_bytes()).expect("fixture should be writable");

    let loaded = load_source(path.to_str().expect("unicode path should be valid UTF-8"))
        .expect("source should load");
    assert_eq!(loaded.source.as_bytes(), source.as_bytes());
    assert_eq!(loaded.extension, "ts");

    fs::remove_dir_all(directory).expect("test directory should be removable");
}

#[test]
fn distinguishes_unsupported_and_missing_files() {
    let directory = test_directory();
    let unsupported = directory.join("notes.py");
    let unsupported_error = load_source(unsupported.to_str().expect("path should be valid UTF-8"))
        .expect_err("unsupported files should not be parsed");
    assert!(matches!(
        unsupported_error,
        CodeScopeError::UnsupportedExtension { .. }
    ));

    let missing = directory.join("missing.ts");
    let missing_error = load_source(missing.to_str().expect("path should be valid UTF-8"))
        .expect_err("missing files should return a structured error");
    assert!(matches!(missing_error, CodeScopeError::FileNotFound { .. }));
    assert!(!is_supported(&unsupported));

    fs::remove_dir_all(directory).expect("test directory should be removable");
}

#[test]
fn rejects_non_utf8_source() {
    let directory = test_directory();
    let path = directory.join("invalid.ts");
    fs::write(&path, [0xff, 0xfe, 0xfd]).expect("fixture should be writable");

    let error = load_source(path.to_str().expect("path should be valid UTF-8"))
        .expect_err("invalid UTF-8 should not be silently replaced");
    assert!(matches!(error, CodeScopeError::ReadFile { .. }));

    fs::remove_dir_all(directory).expect("test directory should be removable");
}

#[cfg(unix)]
#[test]
fn reports_permission_failures() {
    use std::os::unix::fs::PermissionsExt;

    let directory = test_directory();
    let path = directory.join("private.ts");
    fs::write(&path, "export const value = 1;").expect("fixture should be writable");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o000))
        .expect("fixture permissions should be changeable");

    let error = load_source(path.to_str().expect("path should be valid UTF-8"))
        .expect_err("permission failures should be returned");
    assert!(matches!(error, CodeScopeError::ReadFile { .. }));

    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
        .expect("fixture permissions should be restorable");
    fs::remove_dir_all(directory).expect("test directory should be removable");
}
