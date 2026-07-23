mod support;

use std::path::PathBuf;

use proptest::{
    prelude::*,
    test_runner::{Config, FileFailurePersistence, TestCaseError, TestRunner},
};
use support::conformance::{
    assert_corpus_file, assert_generated_case, assert_isolated_corpus_file,
    assert_outline_reads_back, assert_semantic_case, render_case, BindingShape, CallbackShape,
    CaseSpec, ContainerShape, FormattingShape, Language,
};
use symbolpeek::language::{
    go::GoAdapter, java::JavaAdapter, markdown::MarkdownAdapter, python::PythonAdapter,
    rust::RustAdapter, LanguageAdapter,
};

fn case_strategy() -> impl Strategy<Value = CaseSpec> {
    let identifier = || {
        proptest::collection::vec(0_u8..26, 1..12).prop_map(|characters| {
            characters
                .into_iter()
                .map(|value| char::from(b'a' + value))
                .collect::<String>()
        })
    };
    (
        0_u8..4,
        0_u8..5,
        0_u8..4,
        0_u8..3,
        0_u8..4,
        identifier(),
        identifier(),
        1_usize..6,
    )
        .prop_map(
            |(
                language,
                binding,
                callback,
                container,
                formatting,
                property,
                callback_name,
                depth,
            )| CaseSpec {
                language: match language {
                    0 => Language::JavaScript,
                    1 => Language::Jsx,
                    2 => Language::TypeScript,
                    _ => Language::Tsx,
                },
                binding: match binding {
                    0 => BindingShape::Direct,
                    1 => BindingShape::SingleAlias,
                    2 => BindingShape::ObjectFirst,
                    3 => BindingShape::ObjectLast,
                    _ => BindingShape::Tuple,
                },
                callback: match callback {
                    0 => CallbackShape::Arrow,
                    1 => CallbackShape::FunctionExpression,
                    2 => CallbackShape::Method,
                    _ => CallbackShape::JsxArrow,
                },
                container: match container {
                    0 => ContainerShape::Function,
                    1 => ContainerShape::Arrow,
                    _ => ContainerShape::NestedFunction,
                },
                formatting: match formatting {
                    0 => FormattingShape::Multiline,
                    1 => FormattingShape::Compact,
                    2 => FormattingShape::Commented,
                    _ => FormattingShape::Unicode,
                },
                operation_property: format!("x{property}"),
                callback_name: format!("onX{callback_name}"),
                nesting_depth: depth,
            },
        )
}

#[test]
fn generated_symbol_shapes_obey_cross_tool_contracts() {
    let cases = std::env::var("SYMBOLPEEK_CONFORMANCE_CASES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(24);
    let mut runner = TestRunner::new(Config {
        cases,
        max_shrink_iters: 512,
        failure_persistence: Some(Box::new(FileFailurePersistence::Direct(
            "proptest-regressions/conformance.txt",
        ))),
        ..Config::default()
    });
    runner
        .run(&case_strategy(), |spec| {
            let case = render_case(&spec);
            assert_generated_case(&case).map_err(TestCaseError::fail)
        })
        .expect("generated conformance case failed");
}

#[test]
fn all_modeled_symbol_shape_combinations_obey_the_contract() {
    for language in [
        Language::JavaScript,
        Language::Jsx,
        Language::TypeScript,
        Language::Tsx,
    ] {
        for binding in [
            BindingShape::Direct,
            BindingShape::SingleAlias,
            BindingShape::ObjectFirst,
            BindingShape::ObjectLast,
            BindingShape::Tuple,
        ] {
            let properties = match binding {
                BindingShape::Direct | BindingShape::Tuple => &["mutate"][..],
                _ => &["mutate", "trigger", "mutateAsync", "executeLater"][..],
            };
            for property in properties {
                for callback in [
                    CallbackShape::Arrow,
                    CallbackShape::FunctionExpression,
                    CallbackShape::Method,
                    CallbackShape::JsxArrow,
                ] {
                    for container in [
                        ContainerShape::Function,
                        ContainerShape::Arrow,
                        ContainerShape::NestedFunction,
                    ] {
                        for formatting in [
                            FormattingShape::Multiline,
                            FormattingShape::Compact,
                            FormattingShape::Commented,
                            FormattingShape::Unicode,
                        ] {
                            let spec = CaseSpec {
                                language,
                                binding,
                                callback,
                                container,
                                formatting,
                                operation_property: (*property).to_owned(),
                                callback_name: "onSuccess".to_owned(),
                                nesting_depth: 2,
                            };
                            assert_generated_case(&render_case(&spec))
                                .unwrap_or_else(|error| panic!("{spec:?}: {error}"));
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn representative_symbols_work_across_semantic_tools() {
    for (language, binding, property, callback) in [
        (
            Language::JavaScript,
            BindingShape::ObjectFirst,
            "trigger",
            "onSuccess",
        ),
        (
            Language::TypeScript,
            BindingShape::ObjectLast,
            "mutateAsync",
            "onSettled",
        ),
        (Language::Tsx, BindingShape::Tuple, "unused", "onCompleted"),
        (
            Language::Jsx,
            BindingShape::SingleAlias,
            "executeLater",
            "onDone",
        ),
    ] {
        let spec = CaseSpec {
            language,
            binding,
            callback: CallbackShape::Arrow,
            container: ContainerShape::NestedFunction,
            formatting: FormattingShape::Commented,
            operation_property: property.to_owned(),
            callback_name: callback.to_owned(),
            nesting_depth: 3,
        };
        assert_semantic_case(&render_case(&spec))
            .unwrap_or_else(|error| panic!("{spec:?}: {error}"));
    }
}

#[test]
fn curated_real_world_corpus_obeys_cross_tool_contracts() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    for (relative, extension) in [
        ("sample.tsx", "tsx"),
        ("react/real_world.tsx", "tsx"),
        ("typescript/advanced.ts", "ts"),
        ("javascript/modules.js", "js"),
        ("navigation/class_fields.ts", "ts"),
        ("navigation/duplicate_callbacks.ts", "ts"),
        ("navigation/mutation_callbacks.tsx", "tsx"),
    ] {
        let path = root.join(relative);
        assert_corpus_file(&path, extension)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    }
}

/// The same reachability contract the TypeScript suite enforces, applied to
/// every other language. Each fixture holds declarations that legitimately
/// share a qualified name (Java overloads, several Go `init`s, `#[cfg]`-gated
/// Rust twins) or hide behind control flow (Python), which is exactly where a
/// name reported by one tool used to be unreadable by another.
#[test]
fn every_language_reports_only_readable_names() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let cases: [(&dyn LanguageAdapter, &str, &str); 10] = [
        // Kept out of `fixtures/rust`, whose contents an end-to-end workspace
        // search counts.
        (&RustAdapter::new(), "reachability/cfg_twins.rs", "rs"),
        (&RustAdapter::new(), "rust/sample.rs", "rs"),
        (
            &PythonAdapter::new(),
            "python/conditional_definitions.py",
            "py",
        ),
        // A class rebinding plus a property getter/setter pair — nested
        // duplicate leaves that a disambiguated parent could strand.
        (&PythonAdapter::new(), "python/property_pairs.py", "py"),
        (&GoAdapter::new(), "go/duplicate_init.go", "go"),
        // Declarations sharing one source position (`var _, _ = …`), a method
        // leaf colliding with a top-level const, and a bare blank colliding
        // with disambiguated blank siblings.
        (&GoAdapter::new(), "go/blank_collisions.go", "go"),
        (&JavaAdapter::new(), "java/Overloads.java", "java"),
        (&MarkdownAdapter::new(), "markdown/handbook.md", "md"),
        (&MarkdownAdapter::new(), "markdown/setext.md", "md"),
        // The project's own README is the largest real Markdown available.
        (&MarkdownAdapter::new(), "../../README.md", "md"),
    ];
    for (adapter, relative, extension) in cases {
        let path = root.join(relative);
        assert_outline_reads_back(adapter, &path, extension)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    }
}

/// This repository is the only large body of real Rust that ships with the
/// project, and unlike a fixture nobody wrote it to satisfy the indexer.
#[test]
fn own_rust_sources_report_only_readable_names() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut sources = Vec::new();
    collect_rust_sources(&root, &mut sources);
    sources.sort();
    assert!(
        sources.len() > 5,
        "expected this crate's sources, found {}",
        sources.len()
    );
    let adapter = RustAdapter::new();
    for path in sources {
        assert_outline_reads_back(&adapter, &path, "rs")
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    }
}

/// The Go standard library is thousands of files of real third-party Go —
/// generated unicode tables, `//go:build`-gated twins, several `init`s per
/// package — none of it written to satisfy this indexer. Skips gracefully when
/// no Go toolchain is installed, exactly like the TypeScript corpus.
#[test]
fn go_standard_library_corpus_reports_only_readable_names() {
    let Some(goroot) = toolchain_corpus_dir("go", &["env", "GOROOT"]) else {
        eprintln!("skipping Go corpus: no `go` toolchain on PATH");
        return;
    };
    let root = goroot.join("src");
    if !root.is_dir() {
        eprintln!("skipping Go corpus: {} is missing", root.display());
        return;
    }
    let checked = assert_corpus_reads_back(&GoAdapter::new(), &root, "go");
    assert!(
        checked > 500,
        "expected the Go standard library, checked only {checked} files in {}",
        root.display()
    );
}

/// The Python standard library exercises the control-flow branch that regressed
/// (`def`/`class` nested in `if`/`try`/`with`), across code nobody wrote for the
/// indexer. Skips gracefully when no `python3` is installed.
#[test]
fn python_standard_library_corpus_reports_only_readable_names() {
    let Some(stdlib) = toolchain_corpus_dir(
        "python3",
        &["-c", "import sysconfig; print(sysconfig.get_path('stdlib'))"],
    ) else {
        eprintln!("skipping Python corpus: no `python3` on PATH");
        return;
    };
    let checked = assert_corpus_reads_back(&PythonAdapter::new(), &stdlib, "py");
    assert!(
        checked > 200,
        "expected the Python standard library, checked only {checked} files in {}",
        stdlib.display()
    );
}

fn collect_rust_sources(directory: &PathBuf, output: &mut Vec<PathBuf>) {
    collect_sources(directory, "rs", output);
}

fn collect_sources(directory: &PathBuf, extension: &str, output: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            collect_sources(&path, extension, output);
        } else if path.extension().is_some_and(|found| found == extension) {
            output.push(path);
        }
    }
}

/// Runs the reachability contract over every source file under `root`, skipping
/// files that are not valid UTF-8 (some standard libraries ship such fixtures).
/// Returns the number of files actually checked so the caller can assert the
/// corpus was real and not silently empty.
fn assert_corpus_reads_back(
    adapter: &dyn LanguageAdapter,
    root: &PathBuf,
    extension: &str,
) -> usize {
    let mut sources = Vec::new();
    collect_sources(root, extension, &mut sources);
    sources.sort();
    let mut checked = 0;
    for path in sources {
        if std::fs::read_to_string(&path).is_err() {
            continue;
        }
        assert_outline_reads_back(adapter, &path, extension)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        checked += 1;
    }
    checked
}

/// Resolves a toolchain-provided corpus directory by running `program args...`
/// and treating its stdout as a path. Returns `None` (and the caller skips) when
/// the toolchain is absent or the reported directory does not exist — the same
/// graceful skip the TypeScript standard-library corpus uses.
fn toolchain_corpus_dir(program: &str, args: &[&str]) -> Option<PathBuf> {
    let output = std::process::Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let path = PathBuf::from(String::from_utf8(output.stdout).ok()?.trim());
    path.is_dir().then_some(path)
}

/// Hand-written fixtures only cover shapes somebody thought to model, which is
/// the wrong side of the bug this suite exists for. The bundled TypeScript
/// standard library is real third-party declaration code — declaration merging,
/// overload chains, and `interface X` / `declare var X` pairs — and it found
/// symbols that generated cases never produced.
#[test]
fn typescript_standard_library_corpus_obeys_cross_tool_contracts() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("node_modules/typescript/lib");
    let Ok(entries) = std::fs::read_dir(&root) else {
        eprintln!(
            "skipping standard library corpus: {} is missing (run `npm ci`)",
            root.display()
        );
        return;
    };
    let mut files = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("lib.") && name.ends_with(".d.ts"))
        })
        .collect::<Vec<_>>();
    files.sort();
    assert!(
        files.len() > 50,
        "expected the bundled standard library, found {} files in {}",
        files.len(),
        root.display()
    );
    for path in files {
        assert_isolated_corpus_file(&path, "ts")
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    }
}
