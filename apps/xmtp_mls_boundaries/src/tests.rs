//! Fixture crates for each way the check must fail, and the resolution rules
//! that decide what an edge is.

use crate::config::Config;
use crate::graph::Build;
use crate::scip::proto::Occurrence;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const CONFIG: &str = r#"
source = "crates/demo/src"

[modules]
"" = "facade"
"low" = "core"
"high" = "engine"

[items]
"low::engine_part" = "engine"

[allowed]
core = []
engine = ["core"]
facade = ["core", "engine"]

[naming]
prefix = "xmtp_mls_"
exempt = ["xmtp_integration_tests"]
existing = ["demo"]
"#;

const LIB: &str = "pub mod high;\npub mod low;\npub use high::Engine;\n";
const HIGH: &str = "pub mod sub;\npub struct Engine;\npub use crate::low::Low;\n";
const SUB: &str = "pub fn sub() { let _ = super::super::low::Low; }\n";

/// A workspace holding the `demo` crate, whose `low.rs` is `low`, plus
/// `extra` files.
fn workspace(low: &str, extra: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let mut files = vec![
        ("Cargo.toml", "[workspace]\nmembers = [\"crates/*\"]\n"),
        ("crates/demo/Cargo.toml", "[package]\nname = \"demo\"\n"),
        ("crates/demo/src/lib.rs", LIB),
        ("crates/demo/src/high/mod.rs", HIGH),
        ("crates/demo/src/high/sub.rs", SUB),
        ("crates/demo/src/low.rs", low),
    ];
    files.extend_from_slice(extra);
    for (path, text) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    dir
}

fn problems(dir: &Path, config: &str, base: Option<&str>) -> Vec<String> {
    let config = Config::parse(config).unwrap();
    let base = base.map(|base| Config::parse(base).unwrap());
    crate::run(dir, &config, base.as_ref(), None)
        .unwrap()
        .problems
}

fn with_waiver(count: usize) -> String {
    format!(
        "{CONFIG}\n[[waivers]]\nfrom = \"core\"\nto = \"engine\"\ntier = \"path\"\ncount = {count}\nreason = \"fixture\"\n"
    )
}

#[track_caller]
fn assert_one_problem(problems: &[String], needle: &str) {
    assert_eq!(problems.len(), 1, "{problems:#?}");
    assert!(problems[0].contains(needle), "{problems:#?}");
}

#[xmtp_common::test]
fn allowed_edges_pass() {
    let dir = workspace("pub struct Low;\n", &[]);
    assert_eq!(problems(dir.path(), CONFIG, None), Vec::<String>::new());
}

#[xmtp_common::test]
fn an_unlisted_upward_import_fails() {
    let dir = workspace("pub struct Low;\nuse crate::high::Engine;\n", &[]);
    assert_one_problem(
        &problems(dir.path(), CONFIG, None),
        "edge core -> engine is not allowed",
    );
}

#[xmtp_common::test]
fn a_reexport_and_a_macro_body_resolve_to_the_defining_module() {
    for low in [
        "pub struct Low;\nuse crate::Engine;\n",
        "pub struct Low;\nfn f() { let _ = matches!(x, crate::high::Engine); }\n",
        "pub struct Low;\nfn f() -> crate::high::Engine { todo!() }\n",
    ] {
        let dir = workspace(low, &[]);
        assert_one_problem(
            &problems(dir.path(), CONFIG, None),
            "edge core -> engine is not allowed",
        );
    }
}

#[xmtp_common::test]
fn a_name_from_a_glob_of_another_crate_is_not_an_edge() {
    let dir = workspace(
        "pub struct Low;
use crate::HashMap;
",
        &[(
            "crates/demo/src/lib.rs",
            "pub mod high;
pub mod low;
pub use std::collections::*;
",
        )],
    );
    assert_eq!(problems(dir.path(), CONFIG, None), Vec::<String>::new());
}

#[xmtp_common::test]
fn test_suites_are_not_read_but_test_helpers_and_hooks_are() {
    let suite = "pub struct Low;\n#[cfg(all(test, unix))]\nmod tests { use crate::high::Engine; #[test] fn t() {} }\n";
    let dir = workspace(suite, &[]);
    assert_eq!(problems(dir.path(), CONFIG, None), Vec::<String>::new());
    let suite_file = "pub struct Low;\n#[cfg(test)]\nmod tests;\n";
    let dir = workspace(
        suite_file,
        &[(
            "crates/demo/src/low/tests.rs",
            "use crate::high::Engine;\nmod cases { #[xmtp_common::test] fn t() {} }\n",
        )],
    );
    assert_eq!(problems(dir.path(), CONFIG, None), Vec::<String>::new());
    for code in [
        "pub struct Low;\n#[cfg(test)]\nfn helper() -> crate::high::Engine { todo!() }\n",
        "pub struct Low;\n#[cfg(test)]\nmod test_hooks { pub fn hook() -> crate::high::Engine { todo!() } }\n",
    ] {
        let dir = workspace(code, &[]);
        assert_one_problem(
            &problems(dir.path(), CONFIG, None),
            "edge core -> engine is not allowed",
        );
    }
}

#[xmtp_common::test]
fn a_path_through_an_imported_name_is_followed() {
    for code in [
        "pub struct Low;\npub fn engine_part() {}\nfn f() { use crate::low as l; use l::engine_part; engine_part(); }\n",
        "pub struct Low;\npub fn engine_part() {}\nfn f() { use l::engine_part; use crate::low as l; engine_part(); }\n",
        "pub struct Low;\npub fn engine_part() {}\nfn f() { use crate::low as l; l::engine_part(); }\n",
        "pub struct Low;\npub fn engine_part() {}\nuse crate::low as l;\nfn f() { l::engine_part(); }\n",
    ] {
        let dir = workspace(code, &[]);
        assert_one_problem(
            &problems(dir.path(), CONFIG, None),
            "edge core -> engine is not allowed (path tier, 1 references)",
        );
    }
    let same_crate =
        "pub struct Low;\nuse crate::high::Engine;\nfn f() { let _ = Engine::new(); }\n";
    let dir = workspace(same_crate, &[]);
    assert_one_problem(
        &problems(dir.path(), CONFIG, None),
        "edge core -> engine is not allowed (path tier, 1 references)",
    );
}

#[xmtp_common::test]
fn an_item_override_moves_its_body_to_its_crate() {
    let dir = workspace(
        "pub struct Low;\npub fn engine_part() { let _ = crate::high::Engine; }\n",
        &[],
    );
    assert_eq!(problems(dir.path(), CONFIG, None), Vec::<String>::new());
}

#[xmtp_common::test]
fn a_waiver_must_match_the_measured_count() {
    let dir = workspace("pub struct Low;\nuse crate::high::Engine;\n", &[]);
    assert_eq!(
        problems(dir.path(), &with_waiver(1), None),
        Vec::<String>::new()
    );
    assert_one_problem(
        &problems(dir.path(), &with_waiver(2), None),
        "lower it to 1",
    );
    let two = workspace(
        "pub struct Low;\nuse crate::high::Engine;\nuse crate::Engine as Again;\n",
        &[],
    );
    assert_one_problem(
        &problems(two.path(), &with_waiver(1), None),
        "has 2 path references but its waiver allows 1",
    );
}

#[xmtp_common::test]
fn a_waiver_raised_above_the_base_fails() {
    let dir = workspace(
        "pub struct Low;\nuse crate::high::Engine;\nuse crate::Engine as Again;\n",
        &[],
    );
    assert_one_problem(
        &problems(dir.path(), &with_waiver(2), Some(&with_waiver(1))),
        "rose from 1 to 2",
    );
    assert_one_problem(
        &problems(dir.path(), &with_waiver(2), Some(CONFIG)),
        "is new; the base has no such waiver",
    );
}

#[xmtp_common::test]
fn a_waiver_with_no_references_fails() {
    let dir = workspace("pub struct Low;\n", &[]);
    assert_one_problem(
        &problems(dir.path(), &with_waiver(1), None),
        "has no references left; remove it",
    );
    let error = Config::parse(&with_waiver(0)).unwrap_err();
    assert!(error.to_string().contains("has count 0"), "{error}");
}

#[xmtp_common::test]
fn a_new_crate_must_keep_the_log_prefix() {
    let crate_named = |name: &str| {
        workspace(
            "pub struct Low;\n",
            &[(
                Box::leak(format!("crates/{name}/Cargo.toml").into_boxed_str()),
                Box::leak(format!("[package]\nname = \"{name}\"\n").into_boxed_str()),
            )],
        )
    };
    let dir = crate_named("xmtp_client_core");
    assert_one_problem(
        &problems(dir.path(), CONFIG, None),
        "package \"xmtp_client_core\" must start with \"xmtp_mls_\"",
    );
    for allowed in ["xmtp_mls_core", "xmtp_integration_tests"] {
        let dir = crate_named(allowed);
        assert_eq!(problems(dir.path(), CONFIG, None), Vec::<String>::new());
    }
}

/// One occurrence of `symbol` at `line` (0-based) of a fixture document; a
/// definition's body is its line.
fn occurrence(line: i32, symbol: &str, definition: bool) -> Occurrence {
    Occurrence {
        range: vec![line, 0, 1],
        symbol: format!("rust-analyzer cargo demo 0.1.0 {symbol}"),
        symbol_roles: i32::from(definition),
        enclosing_range: if definition {
            vec![line, 0, line, 1000]
        } else {
            vec![]
        },
    }
}

/// A reference to the method `symbol` names, at its first `.name(` on
/// `line` of `code`.
fn call(code: &str, line: i32, symbol: &str) -> Occurrence {
    let name = symbol
        .trim_end_matches("().")
        .rsplit(['/', '#', ']'])
        .next()
        .unwrap();
    let text = code.lines().nth(line as usize).unwrap();
    let column = text.find(&format!(".{name}(")).unwrap() + 1;
    Occurrence {
        range: vec![line, column as i32, (column + name.len()) as i32],
        ..occurrence(line, symbol, false)
    }
}

/// A definition whose body spans `start` to `end`.
fn with_body(mut definition: Occurrence, start: (i32, i32), end: (i32, i32)) -> Occurrence {
    definition.enclosing_range = vec![start.0, start.1, end.0, end.1];
    definition
}

/// Write the SCIP index `name`, from `tool`, holding a document for each
/// module file of the fixture crate except `skip`, with `low` as the
/// occurrences of `low.rs`.
fn index(dir: &Path, name: &str, tool: &str, skip: Option<&str>, low: &[Occurrence]) -> PathBuf {
    use crate::scip::proto::{Document, Index, Metadata, ToolInfo};
    use prost::Message;
    let documents = ["lib.rs", "high/mod.rs", "high/sub.rs", "low.rs"]
        .into_iter()
        .filter(|file| Some(*file) != skip)
        .map(|file| Document {
            relative_path: format!("crates/demo/src/{file}"),
            occurrences: if file == "low.rs" {
                low.to_vec()
            } else {
                vec![]
            },
            position_encoding: 1,
        })
        .collect();
    let index = Index {
        metadata: Some(Metadata {
            tool_info: Some(ToolInfo {
                name: tool.into(),
                version: "fixture".into(),
            }),
            project_root: format!("file://{}", dir.display()),
        }),
        documents,
    };
    let path = dir.join(name);
    std::fs::write(&path, index.encode_to_vec()).unwrap();
    path
}

/// rust-analyzer indexes of both builds, with `native` and `wasm` as the
/// occurrences of `low.rs`.
fn indexes(dir: &Path, native: &[Occurrence], wasm: &[Occurrence]) -> BTreeMap<Build, PathBuf> {
    BTreeMap::from([
        (
            Build::NativeTest,
            index(dir, "native.scip", "rust-analyzer", None, native),
        ),
        (
            Build::WasmProduction,
            index(dir, "wasm.scip", "rust-analyzer", None, wasm),
        ),
    ])
}

const CALLER: &str = "pub struct Low;\npub fn low(engine: Engine) { engine.run(); }\n";

fn calls_into_engine() -> Vec<Occurrence> {
    vec![
        occurrence(1, "low/low().", true),
        call(CALLER, 1, "high/Engine#run()."),
    ]
}

fn call_tier(
    dir: &Path,
    config: &str,
    indexes: &BTreeMap<Build, PathBuf>,
) -> anyhow::Result<Vec<String>> {
    let config = Config::parse(config).unwrap();
    crate::run(dir, &config, None, Some(indexes)).map(|outcome| outcome.problems)
}

#[xmtp_common::test]
fn a_method_call_into_a_forbidden_module_fails_the_call_tier() {
    let dir = workspace(CALLER, &[]);
    let calls = calls_into_engine();
    assert_one_problem(
        &call_tier(dir.path(), CONFIG, &indexes(dir.path(), &calls, &calls)).unwrap(),
        "edge core -> engine is not allowed (call tier, 1 references)",
    );
}

#[xmtp_common::test]
fn an_index_missing_a_module_file_fails_closed() {
    let dir = workspace(CALLER, &[]);
    let calls = calls_into_engine();
    let mut indexes = indexes(dir.path(), &calls, &calls);
    indexes.insert(
        Build::NativeTest,
        index(
            dir.path(),
            "partial.scip",
            "rust-analyzer",
            Some("high/sub.rs"),
            &calls,
        ),
    );
    let error = call_tier(dir.path(), CONFIG, &indexes)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("the NativeTest index has no document for"),
        "{error}"
    );
}

#[xmtp_common::test]
fn an_index_resolving_fewer_method_calls_than_the_syntax_fails_closed() {
    let dir = workspace(CALLER, &[]);
    let indexes = indexes(
        dir.path(),
        &[occurrence(1, "low/low().", true)],
        &calls_into_engine(),
    );
    let error = call_tier(dir.path(), CONFIG, &indexes)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("the NativeTest index does not resolve 1 method calls"),
        "{error}"
    );
}

#[xmtp_common::test]
fn a_call_on_another_line_does_not_stand_in_for_an_unresolved_one() {
    let dir = workspace(&format!("{CALLER}pub fn ufcs() {{ Low::run(); }}\n"), &[]);
    let ufcs = vec![
        occurrence(1, "low/low().", true),
        occurrence(2, "low/ufcs().", true),
        occurrence(2, "low/Low#run().", false),
    ];
    let error = call_tier(dir.path(), CONFIG, &indexes(dir.path(), &ufcs, &ufcs))
        .unwrap_err()
        .to_string();
    assert!(error.contains("low.rs:2 .run() x1"), "{error}");
}

#[xmtp_common::test]
fn an_index_from_another_tool_or_a_missing_build_is_refused() {
    let dir = workspace(CALLER, &[]);
    let calls = calls_into_engine();
    let mut indexes = indexes(dir.path(), &calls, &calls);
    indexes.insert(
        Build::WasmProduction,
        index(dir.path(), "other.scip", "scip-other", None, &calls),
    );
    let error = call_tier(dir.path(), CONFIG, &indexes)
        .unwrap_err()
        .to_string();
    assert!(error.contains("not rust-analyzer"), "{error}");
    indexes.remove(&Build::WasmProduction);
    let error = call_tier(dir.path(), CONFIG, &indexes)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("needs a SCIP index of the WasmProduction build"),
        "{error}"
    );
}

#[xmtp_common::test]
fn a_method_of_an_impl_belongs_to_its_self_type() {
    use crate::scip::parse_symbol;
    for (symbol, module, item) in [
        (
            "rust-analyzer cargo core https://github.com/rust-lang/rust/library/core slice/impl#[`[T]`]iter().",
            vec!["slice"],
            vec!["[T]", "iter"],
        ),
        (
            "rust-analyzer cargo demo 0.1.0 high/impl#[`Engine<``T``>`][Run]run().",
            vec!["high"],
            vec!["Engine", "run"],
        ),
        (
            "rust-analyzer cargo demo 0.1.0 low/impl#[Low]run().",
            vec!["low"],
            vec!["Low", "run"],
        ),
    ] {
        let parsed = parse_symbol(symbol).unwrap();
        assert!(parsed.callable, "{symbol}: {parsed:?}");
        assert_eq!(parsed.module, module, "{symbol}");
        assert_eq!(parsed.item, item, "{symbol}");
    }
}

#[xmtp_common::test]
fn an_override_of_an_impl_method_applies_to_its_calls() {
    let config = CONFIG.replace(
        "\"low::engine_part\" = \"engine\"",
        "\"low::engine_part\" = \"engine\"\n\"low::Low::run\" = \"engine\"",
    );
    let low = "pub struct Low;\nimpl Low { pub fn run(&self) {} }\npub fn caller(low: Low) { low.run(); }\n";
    let dir = workspace(low, &[]);
    let calls = vec![
        occurrence(1, "low/impl#[Low]run().", true),
        occurrence(2, "low/caller().", true),
        call(low, 2, "low/impl#[Low]run()."),
    ];
    assert_one_problem(
        &call_tier(dir.path(), &config, &indexes(dir.path(), &calls, &calls)).unwrap(),
        "edge core -> engine is not allowed (call tier, 1 references)",
    );
}

#[xmtp_common::test]
fn a_call_only_another_build_compiles_must_resolve_in_its_index() {
    let low = format!(
        "{CALLER}#[cfg(target_arch = \"wasm32\")]\npub fn web(engine: Engine) {{ engine.web(); }}\npub fn both(engine: Engine) {{\n    #[cfg(not(test))]\n    engine.prod();\n}}\n#[cfg(all(test, target_arch = \"wasm32\"))]\nfn neither(engine: Engine) {{ engine.nope(); }}\n"
    );
    let dir = workspace(&low, &[]);
    let native = calls_into_engine();
    let mut wasm = calls_into_engine();
    wasm.extend([
        occurrence(3, "low/web().", true),
        call(&low, 3, "high/Engine#web()."),
        occurrence(4, "low/both().", true),
    ]);
    let error = call_tier(dir.path(), CONFIG, &indexes(dir.path(), &native, &wasm))
        .unwrap_err()
        .to_string();
    assert!(error.contains("low.rs:7 .prod() x1"), "{error}");
    wasm.push(call(&low, 6, "high/Engine#prod()."));
    let error = call_tier(dir.path(), CONFIG, &indexes(dir.path(), &native, &wasm))
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("uncovered does not list these method calls, which no indexed build compiles:\n    crates/demo/src/low.rs:10 .nope()"),
        "{error}"
    );
    let listing = |methods: &[&str]| {
        methods
            .iter()
            .map(|method| format!("\n[[uncovered]]\nfile = \"crates/demo/src/low.rs\"\nmethod = \"{method}\"\nreason = \"fixture\"\n"))
            .collect::<String>()
    };
    let config = format!("{CONFIG}{}", listing(&["nope"]));
    assert_one_problem(
        &call_tier(dir.path(), &config, &indexes(dir.path(), &native, &wasm)).unwrap(),
        "edge core -> engine is not allowed (call tier, 3 references)",
    );
    for methods in [&["nope", "nope"][..], &["other"]] {
        let config = format!("{CONFIG}{}", listing(methods));
        let error = call_tier(dir.path(), &config, &indexes(dir.path(), &native, &wasm))
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("uncovered lists calls that every indexed build misses no longer"),
            "{error}"
        );
    }
    let base = Config::parse(&config).unwrap();
    let raised = Config::parse(&format!("{CONFIG}{}", listing(&["nope", "nope"]))).unwrap();
    assert_eq!(
        raised.loosened_from(&base),
        vec![
            "uncovered lists crates/demo/src/low.rs .nope() 2 times; the base lists it 1"
                .to_string()
        ]
    );
}

#[xmtp_common::test]
fn references_inside_a_test_suite_are_not_read_by_the_call_tier() {
    for suite in [
        "#[cfg(test)]\nmod tests {\n    #[test]\n    fn check(engine: crate::high::Engine) { engine.check(); }\n}\n",
        "xmtp_common::if_only_test! {\nmod tests {\n    #[test]\n    fn check(engine: crate::high::Engine) { engine.check(); }\n}\n}\n",
    ] {
        let low = format!("{CALLER}{suite}");
        let dir = workspace(&low, &[]);
        let mut native = calls_into_engine();
        native.extend([
            occurrence(5, "low/tests/check().", true),
            occurrence(5, "high/Engine#", false),
            occurrence(5, "high/Engine#check().", false),
        ]);
        assert_one_problem(
            &call_tier(
                dir.path(),
                CONFIG,
                &indexes(dir.path(), &native, &calls_into_engine()),
            )
            .unwrap(),
            "edge core -> engine is not allowed (call tier, 1 references)",
        );
    }
}

#[xmtp_common::test]
fn a_helper_before_a_suite_in_one_macro_is_read_by_the_call_tier() {
    let low = format!(
        "{CALLER}xmtp_common::if_only_test! {{\nfn helper(engine: crate::high::Engine) {{ engine.help(); }}\nmod tests {{\n    #[test]\n    fn check(engine: crate::high::Engine) {{ engine.check(); }}\n}}\n}}\n"
    );
    let dir = workspace(&low, &[]);
    let mut native = calls_into_engine();
    native.extend([
        occurrence(3, "low/helper().", true),
        call(&low, 3, "high/Engine#help()."),
        occurrence(6, "low/tests/check().", true),
        occurrence(6, "high/Engine#check().", false),
    ]);
    let problems = call_tier(
        dir.path(),
        CONFIG,
        &indexes(dir.path(), &native, &calls_into_engine()),
    )
    .unwrap();
    assert!(
        problems.iter().any(|problem| problem.contains(
            "edge core -> engine is not allowed (call tier, 2 references):\n    crates/demo/src/low.rs high::Engine::run\n    crates/demo/src/low.rs high::Engine::help"
        )),
        "{problems:#?}"
    );
}

#[xmtp_common::test]
fn a_local_macro_named_like_a_non_evaluating_one_is_refused() {
    for (low, needle) in [
        (
            "pub struct Low;\nfn f() {\n    macro_rules! stringify { ($e:expr) => { $e }; }\n}\n",
            "low.rs:3 defines a macro named stringify",
        ),
        (
            "pub struct Low;\nother_macro! { macro_rules! concat { () => {} } }\n",
            "low.rs:2 defines a macro named concat",
        ),
        (
            "pub struct Low;\nmacro_rules! r#stringify { ($e:expr) => { $e }; }\n",
            "low.rs:2 defines a macro named r#stringify",
        ),
        (
            "pub struct Low;\nother_macro! { macro_rules! r#concat { () => {} } }\n",
            "low.rs:2 defines a macro named r#concat",
        ),
        (
            "pub struct Low;\nfn f() {\n    use pass as r#stringify;\n}\n",
            "low.rs:3 imports pass as stringify",
        ),
        (
            "pub struct Low;\nfn f() {\n    use core::assert as r#stringify;\n}\n",
            "low.rs:3 imports core::assert as stringify",
        ),
        (
            "pub struct Low;\nuse xmtp_mls_eval as r#core;\n",
            "low.rs:2 imports xmtp_mls_eval as core",
        ),
        (
            "pub struct Low;\nextern crate xmtp_mls_eval as core;\n",
            "low.rs:2 imports the crate xmtp_mls_eval as core",
        ),
    ] {
        let dir = workspace(low, &[]);
        let config = Config::parse(CONFIG).unwrap();
        let error = crate::run(dir.path(), &config, None, None)
            .err()
            .unwrap()
            .to_string();
        assert!(error.contains(needle), "{error}");
    }
    let dir = workspace(
        "pub struct Low;\nuse std::env;\nuse core::stringify;\nuse r#core::r#concat;\n",
        &[],
    );
    assert_eq!(problems(dir.path(), CONFIG, None), Vec::<String>::new());
}

#[xmtp_common::test]
fn an_uncovered_call_of_a_method_the_crate_defines_is_refused() {
    let low = format!(
        "{CALLER}impl Low {{ pub fn up(&self) {{}} }}\n#[cfg(all(test, target_arch = \"wasm32\"))]\nfn neither(low: Low) {{ low.up(); }}\n"
    );
    let dir = workspace(&low, &[]);
    let config = format!(
        "{CONFIG}\n[[uncovered]]\nfile = \"crates/demo/src/low.rs\"\nmethod = \"up\"\nreason = \"fixture\"\n"
    );
    let calls = calls_into_engine();
    let error = call_tier(dir.path(), &config, &indexes(dir.path(), &calls, &calls))
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("low.rs:5 .up() is in code no indexed build compiles, and the checked crate defines low::Low::up"),
        "{error}"
    );
}

#[xmtp_common::test]
fn module_level_code_after_an_item_belongs_to_the_module() {
    let low = "pub struct Low;\npub fn engine_part() {}\nuse crate::high::Engine;\n";
    let dir = workspace(low, &[]);
    let occurrences = vec![
        with_body(occurrence(1, "low/engine_part().", true), (1, 0), (1, 23)),
        occurrence(2, "high/Engine#", false),
    ];
    let problems = call_tier(
        dir.path(),
        CONFIG,
        &indexes(dir.path(), &occurrences, &occurrences),
    )
    .unwrap();
    assert_eq!(problems.len(), 2, "{problems:#?}");
    assert!(
        problems.iter().any(|problem| problem
            .contains("edge core -> engine is not allowed (call tier, 1 references)")),
        "{problems:#?}"
    );
}

#[xmtp_common::test]
fn a_test_hook_module_is_read_by_the_call_tier() {
    let low = format!(
        "{CALLER}#[cfg(test)]\nmod test_hooks {{ pub fn hook(engine: Engine) {{ engine.run(); }} }}\n"
    );
    let dir = workspace(&low, &[]);
    let mut native = calls_into_engine();
    native.extend([
        occurrence(3, "low/test_hooks/hook().", true),
        call(&low, 3, "high/Engine#run()."),
    ]);
    assert_one_problem(
        &call_tier(
            dir.path(),
            CONFIG,
            &indexes(dir.path(), &native, &calls_into_engine()),
        )
        .unwrap(),
        "edge core -> engine is not allowed (call tier, 2 references)",
    );
}

#[xmtp_common::test]
fn a_same_line_reference_does_not_stand_in_for_an_unresolved_call() {
    let low = "pub struct Low;\npub struct Other;\nimpl Other { pub fn run() {} }\npub fn low(engine: Engine) { engine.run(); Other::run(); }\n";
    let dir = workspace(low, &[]);
    let column = low.lines().nth(3).unwrap().find("Other::run").unwrap() as i32 + 7;
    let occurrences = vec![
        occurrence(3, "low/low().", true),
        Occurrence {
            range: vec![3, column, column + 3],
            ..occurrence(3, "low/impl#[Other]run().", false)
        },
    ];
    let error = call_tier(
        dir.path(),
        CONFIG,
        &indexes(dir.path(), &occurrences, &occurrences),
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("low.rs:4 .run() x1"), "{error}");
}

#[xmtp_common::test]
fn a_method_call_in_macro_arguments_must_resolve() {
    let low = "pub struct Low;\npub fn low(engine: Engine) { pass!(engine.run()); }\nxmtp_common::if_wasm! { pub fn web(engine: Engine) { engine.web(); } }\n";
    let dir = workspace(low, &[]);
    let native = vec![occurrence(1, "low/low().", true)];
    let error = call_tier(dir.path(), CONFIG, &indexes(dir.path(), &native, &native))
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("the NativeTest index does not resolve 1 method calls"),
        "{error}"
    );
    assert!(error.contains("low.rs:3 .web() x1"), "{error}");
    let native = vec![
        occurrence(1, "low/low().", true),
        call(low, 1, "high/Engine#run()."),
    ];
    let mut wasm = native.clone();
    wasm.extend([
        occurrence(2, "low/web().", true),
        call(low, 2, "high/Engine#web()."),
    ]);
    assert_one_problem(
        &call_tier(dir.path(), CONFIG, &indexes(dir.path(), &native, &wasm)).unwrap(),
        "edge core -> engine is not allowed (call tier, 2 references)",
    );
}

#[xmtp_common::test]
fn an_index_without_body_ranges_fails_closed() {
    let dir = workspace(CALLER, &[]);
    let mut calls = calls_into_engine();
    calls[0].enclosing_range.clear();
    let error = call_tier(dir.path(), CONFIG, &indexes(dir.path(), &calls, &calls))
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("index gives no body range for 1 definitions"),
        "{error}"
    );
}

#[xmtp_common::test]
fn a_module_inside_a_block_is_refused() {
    let dir = workspace(
        "pub struct Low;\nfn f() { mod nested { use crate::high::Engine; } }\n",
        &[],
    );
    let config = Config::parse(CONFIG).unwrap();
    let error = crate::run(dir.path(), &config, None, None)
        .err()
        .unwrap()
        .to_string();
    assert!(
        error.contains("low.rs:2 declares a module inside a block"),
        "{error}"
    );
}

#[xmtp_common::test]
fn a_call_resolves_only_at_its_column_in_the_document_encoding() {
    let low = "pub struct Low;\npub struct Other;\npub fn low(low: Low, other: Other) { let _ = \"ééééééééééééé\"; low.run(); other.run(); }\n";
    let dir = workspace(low, &[]);
    let text = low.lines().nth(2).unwrap();
    let first = text.find(".run(").unwrap() + 1;
    let occurrences = vec![
        occurrence(2, "low/low().", true),
        Occurrence {
            range: vec![2, first as i32, first as i32 + 3],
            ..occurrence(2, "low/impl#[Low]run().", false)
        },
    ];
    let error = call_tier(
        dir.path(),
        CONFIG,
        &indexes(dir.path(), &occurrences, &occurrences),
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("low.rs:3 .run() x1"), "{error}");
    let indexes = indexes(dir.path(), &occurrences, &occurrences);
    let path = &indexes[&Build::NativeTest];
    let mut unstated = {
        use prost::Message;
        crate::scip::proto::Index::decode(std::fs::read(path).unwrap().as_slice()).unwrap()
    };
    for document in &mut unstated.documents {
        document.position_encoding = 0;
    }
    std::fs::write(path, prost::Message::encode_to_vec(&unstated)).unwrap();
    let error = call_tier(dir.path(), CONFIG, &indexes)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("does not state the position encoding"),
        "{error}"
    );
}

#[xmtp_common::test]
fn a_module_in_an_item_macro_is_read_and_one_in_another_macro_refused() {
    let low = "pub struct Low;\nxmtp_common::if_native! {\n    mod hidden { fn f(engine: crate::Engine) { engine.run(); } }\n}\n";
    let dir = workspace(low, &[]);
    let native = vec![with_body(
        occurrence(2, "low/hidden/f().", true),
        (2, 0),
        (2, 80),
    )];
    let error = call_tier(dir.path(), CONFIG, &indexes(dir.path(), &native, &[]))
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("the NativeTest index does not resolve 1 method calls"),
        "{error}"
    );
    assert!(
        !error.contains("WasmProduction index does not resolve"),
        "{error}"
    );
    let dir = workspace(
        "pub struct Low;\nother_macro! {\n    mod hidden { fn f(low: crate::low::Low) { low.run(); } }\n}\n",
        &[],
    );
    let config = Config::parse(CONFIG).unwrap();
    let error = crate::run(dir.path(), &config, None, None)
        .err()
        .unwrap()
        .to_string();
    assert!(
        error.contains("low.rs:3 declares a module in macro arguments"),
        "{error}"
    );
}

#[xmtp_common::test]
fn macro_arguments_are_read_as_the_macro_compiles_them() {
    let low = "pub struct Low;\npub fn low(engine: Engine) { let _ = stringify!(engine.run()); assert_eq!(stringify!(engine.run()), \"\"); }\nxmtp_common::if_only_test! { pub fn hook(engine: Engine) { engine.hook(); } }\nconst TEXT: &str = stringify!(mod ignored {});\nconst RAW: &str = r#stringify!(macro_rules! r#concat { () => {} });\n";
    let dir = workspace(low, &[]);
    let wasm = vec![occurrence(1, "low/low().", true)];
    let mut native = wasm.clone();
    native.extend([
        occurrence(2, "low/hook().", true),
        call(low, 2, "high/Engine#hook()."),
    ]);
    assert_one_problem(
        &call_tier(dir.path(), CONFIG, &indexes(dir.path(), &native, &wasm)).unwrap(),
        "edge core -> engine is not allowed (call tier, 1 references)",
    );
}
