//! The `call` tier: every reference to an item in a rust-analyzer SCIP index
//! of the crate, method calls and inferred types included, resolved to the
//! module of its definition. A module named in a path is not an item, and
//! test suites are not read, as in the path tier.
//!
//! It reads one index per build (`graph::Build`), so code that only a wasm
//! or a non-test build compiles is checked too. The tier fails closed. It
//! refuses an index that rust-analyzer did not write, a missing build, an
//! index that lacks a document for a module file the build compiles, and one
//! that resolves fewer calls of a method name on a line than the line holds.

use crate::config::Config;
use crate::graph::{Build, Crate, ModPath, Reference, target_of};
use anyhow::{Context, Result, bail};
use prost::Message;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// The parts of `scip.proto` the tier reads.
pub(crate) mod proto {
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct Index {
        #[prost(message, optional, tag = "1")]
        pub metadata: Option<Metadata>,
        #[prost(message, repeated, tag = "2")]
        pub documents: Vec<Document>,
    }

    #[derive(Clone, PartialEq, prost::Message)]
    pub struct Metadata {
        #[prost(message, optional, tag = "2")]
        pub tool_info: Option<ToolInfo>,
        #[prost(string, tag = "3")]
        pub project_root: String,
    }

    #[derive(Clone, PartialEq, prost::Message)]
    pub struct ToolInfo {
        #[prost(string, tag = "1")]
        pub name: String,
        #[prost(string, tag = "2")]
        pub version: String,
    }

    #[derive(Clone, PartialEq, prost::Message)]
    pub struct Document {
        #[prost(string, tag = "1")]
        pub relative_path: String,
        #[prost(message, repeated, tag = "2")]
        pub occurrences: Vec<Occurrence>,
        /// 1 for UTF-8, 2 for UTF-16 and 3 for UTF-32 column offsets.
        #[prost(int32, tag = "6")]
        pub position_encoding: i32,
    }

    #[derive(Clone, PartialEq, prost::Message)]
    pub struct Occurrence {
        #[prost(int32, repeated, tag = "1")]
        pub range: Vec<i32>,
        #[prost(string, tag = "2")]
        pub symbol: String,
        #[prost(int32, tag = "3")]
        pub symbol_roles: i32,
        #[prost(int32, repeated, tag = "7")]
        pub enclosing_range: Vec<i32>,
    }
}

const DEFINITION: i32 = 0x1;

/// What one SCIP symbol names: its package, the module path of its
/// namespaces, the item names after them, and whether it is a method or
/// function.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Symbol {
    pub(crate) package: String,
    pub(crate) module: ModPath,
    pub(crate) item: Vec<String>,
    pub(crate) callable: bool,
}

/// Read a backtick-escaped name after its opening backtick; a doubled
/// backtick is a literal one.
fn escaped(chars: &mut std::iter::Peekable<std::str::Chars<'_>>, name: &mut String) {
    while let Some(c) = chars.next() {
        if c != '`' {
            name.push(c);
        } else if chars.peek() == Some(&'`') {
            chars.next();
            name.push('`');
        } else {
            return;
        }
    }
}

/// Split a SCIP symbol into its descriptors; `None` for a local symbol.
pub(crate) fn parse_symbol(symbol: &str) -> Option<Symbol> {
    if symbol.starts_with("local ") {
        return None;
    }
    let mut parts = symbol.splitn(5, ' ');
    let (_scheme, _manager, package, _version, descriptors) = (
        parts.next()?,
        parts.next()?,
        parts.next()?,
        parts.next()?,
        parts.next()?,
    );
    let mut module = vec![];
    let mut item = vec![];
    let mut callable = false;
    let mut impl_self_type = false;
    let mut chars = descriptors.chars().peekable();
    let mut name = String::new();
    while let Some(c) = chars.next() {
        match c {
            '`' => escaped(&mut chars, &mut name),
            '/' => module.push(std::mem::take(&mut name)),
            '#' | '.' | '!' | ':' => {
                if !name.is_empty() {
                    impl_self_type = c == '#' && name == "impl";
                    item.push(std::mem::take(&mut name));
                }
            }
            '(' => {
                for c in chars.by_ref() {
                    if c == ')' {
                        break;
                    }
                }
                if chars.peek() == Some(&'.') {
                    chars.next();
                    callable = true;
                    item.push(std::mem::take(&mut name));
                } else {
                    name.clear();
                }
            }
            '[' => {
                let mut content = String::new();
                while let Some(c) = chars.next() {
                    match c {
                        ']' => break,
                        '`' => escaped(&mut chars, &mut content),
                        c => content.push(c),
                    }
                }
                name.clear();
                // `impl#[SelfType][Trait]`: the methods of an impl belong to
                // its self type, as in the path tier.
                if impl_self_type {
                    impl_self_type = false;
                    let self_type = content.split('<').next().unwrap_or_default();
                    let self_type = self_type.rsplit("::").next().unwrap_or_default();
                    if !self_type.is_empty()
                        && let Some(last) = item.last_mut()
                    {
                        *last = self_type.to_string();
                    }
                }
            }
            c => name.push(c),
        }
    }
    Some(Symbol {
        package: package.to_string(),
        module: module
            .into_iter()
            .filter(|segment| !segment.is_empty())
            .collect(),
        item,
        callable,
    })
}

pub struct CallTier {
    pub references: Vec<Reference>,
    pub summary: String,
}

/// A position in a document: 0-based line and character.
type Position = (i32, i32);

/// The start and end of a SCIP range: `[line, start, end]` on one line or
/// `[start line, start, end line, end]`.
fn span(range: &[i32]) -> Option<(Position, Position)> {
    match *range {
        [line, start, end] => Some(((line, start), (line, end))),
        [line, start, end_line, end] => Some(((line, start), (end_line, end))),
        _ => None,
    }
}

/// A definition of the crate in one document and the range of its body.
struct Owner {
    body: (Position, Position),
    symbol: Symbol,
}

/// The innermost definition whose body holds `at`, or none for
/// module-level code.
fn owner_of(owners: &[Owner], at: Position) -> Option<&Symbol> {
    owners
        .iter()
        .filter(|owner| owner.body.0 <= at && at <= owner.body.1)
        .max_by_key(|owner| owner.body.0)
        .map(|owner| &owner.symbol)
}

fn decode(index: &Path) -> Result<(proto::Index, String)> {
    let bytes = std::fs::read(index).with_context(|| format!("reading {}", index.display()))?;
    let index = proto::Index::decode(bytes.as_slice()).context("decoding the SCIP index")?;
    let tool = index
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.tool_info.as_ref())
        .map(|tool| format!("{} {}", tool.name, tool.version))
        .unwrap_or_default();
    if !tool.starts_with("rust-analyzer") {
        bail!("the SCIP index was written by {tool:?}, not rust-analyzer");
    }
    Ok((index, tool))
}

/// The references of the checked crate that the indexes resolve, one index
/// per build. Every build needs its index, every module file the build
/// compiles needs a document, and every method call the build compiles must
/// resolve on its line.
pub fn references(
    config: &Config,
    krate: &Crate,
    workspace: &Path,
    indexes: &BTreeMap<Build, PathBuf>,
) -> Result<CallTier> {
    let package = Path::new(&config.source)
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .context("the configured source has no crate directory")?;
    let method_calls = krate.method_calls();
    // Keyed by occurrence, so code that both builds compile counts once.
    let mut found: BTreeMap<(PathBuf, Position, String), Reference> = BTreeMap::new();
    let mut problems = vec![];
    let mut summaries = vec![];
    for build in Build::ALL {
        let Some(index) = indexes.get(&build) else {
            bail!("the call tier needs a SCIP index of the {build:?} build");
        };
        let (index, tool) = decode(index)?;
        let documents: BTreeMap<PathBuf, &proto::Document> = index
            .documents
            .iter()
            .map(|document| (workspace.join(&document.relative_path), document))
            .collect();
        let expected_calls = method_calls.active.get(&build);
        let files = krate.files_in(build);
        let mut missing = vec![];
        let mut unresolved = BTreeMap::new();
        let mut resolved_calls = 0usize;
        let mut bodiless = 0usize;
        let mut definitions = 0usize;
        for file in &files {
            let Some(document) = documents.get(*file) else {
                missing.push(file.to_path_buf());
                continue;
            };
            let relative = file.strip_prefix(workspace).unwrap_or(file).to_path_buf();
            let module = krate.module_of(file).cloned().unwrap_or_default();
            let in_tests = |occurrence: &proto::Occurrence| {
                occurrence
                    .range
                    .first()
                    .is_some_and(|line| krate.in_test_module(file, *line as usize + 1))
            };
            let mut owners = vec![];
            for occurrence in &document.occurrences {
                if occurrence.symbol_roles & DEFINITION == 0 || in_tests(occurrence) {
                    continue;
                }
                let Some(symbol) = parse_symbol(&occurrence.symbol)
                    .filter(|symbol| symbol.package == package && !symbol.item.is_empty())
                else {
                    continue;
                };
                definitions += 1;
                match span(&occurrence.enclosing_range) {
                    Some(body) => owners.push(Owner { body, symbol }),
                    None => bodiless += 1,
                }
            }
            let mut calls = BTreeSet::new();
            for occurrence in &document.occurrences {
                if occurrence.symbol_roles & DEFINITION != 0 || in_tests(occurrence) {
                    continue;
                }
                let (Some(symbol), Some((at, _))) =
                    (parse_symbol(&occurrence.symbol), span(&occurrence.range))
                else {
                    continue;
                };
                if symbol.callable
                    && let Some(name) = symbol.item.last()
                {
                    calls.insert((at.0 as usize + 1, at.1 as usize, name.clone()));
                }
                if symbol.package != package || symbol.item.is_empty() {
                    continue;
                }
                let from = owner_of(&owners, at).map_or_else(
                    || target_of(config, &module, &[]),
                    |owner| target_of(config, &owner.module, &owner.item),
                );
                let to = target_of(config, &symbol.module, &symbol.item);
                if from != to {
                    let mut path = symbol.module.clone();
                    path.extend(symbol.item.iter().cloned());
                    found
                        .entry((relative.clone(), at, occurrence.symbol.clone()))
                        .or_insert_with(|| Reference {
                            file: relative.clone(),
                            from: from.to_string(),
                            to: to.to_string(),
                            path: path.join("::"),
                        });
                }
            }
            let column_of = |before: &str| match document.position_encoding {
                1 => Some(before.len()),
                2 => Some(before.encode_utf16().count()),
                3 => Some(before.chars().count()),
                _ => None,
            };
            if column_of("").is_none() {
                bail!(
                    "the {build:?} index does not state the position encoding of {}",
                    relative.display()
                );
            }
            let text = std::fs::read_to_string(file)
                .with_context(|| format!("reading {}", file.display()))?;
            let lines: Vec<&str> = text.lines().collect();
            for ((line, column, name), count) in expected_calls
                .and_then(|calls| calls.get(*file))
                .into_iter()
                .flatten()
            {
                // The syntax counts columns in characters, the index in its
                // document's encoding.
                let before = lines
                    .get(line - 1)
                    .map(|text| text.chars().take(*column).collect::<String>())
                    .unwrap_or_default();
                let resolved = column_of(&before)
                    .is_some_and(|offset| calls.contains(&(*line, offset, name.clone())));
                if resolved {
                    resolved_calls += count;
                } else {
                    *unresolved
                        .entry((relative.clone(), *line, name.clone()))
                        .or_default() += count;
                }
            }
        }
        let expected_total: usize = expected_calls
            .into_iter()
            .flat_map(|files| files.values())
            .flat_map(|calls| calls.values())
            .sum();
        let unresolved_total: usize = unresolved.values().sum();
        summaries.push(format!(
            "{build:?} index ({tool}): {} module files, {} documents missing, {resolved_calls}/{expected_total} method calls resolved, {definitions} definitions",
            files.len(),
            missing.len(),
        ));
        if bodiless > 0 {
            problems.push(format!(
                "the {build:?} index gives no body range for {bodiless} definitions"
            ));
        }
        if !missing.is_empty() {
            problems.push(format!(
                "the {build:?} index has no document for:{}",
                missing
                    .iter()
                    .map(|file| format!("\n    {}", file.display()))
                    .collect::<String>()
            ));
        }
        if !unresolved.is_empty() {
            let listing: String = unresolved
                .iter()
                .take(40)
                .map(|((file, line, name), count)| {
                    format!("\n    {}:{line} .{name}() x{count}", file.display())
                })
                .collect();
            problems.push(format!(
                "the {build:?} index does not resolve {unresolved_total} method calls:{listing}"
            ));
        }
    }
    let uncovered = method_calls.uncovered.len();
    let mut listed: Vec<(String, String)> =
        config.uncovered.iter().map(|call| call.key()).collect();
    let mut unlisted = String::new();
    let defined = krate.defined_methods();
    for (file, line, name) in &method_calls.uncovered {
        let file = file.strip_prefix(workspace).unwrap_or(file);
        if let Some(definition) = defined.get(name) {
            problems.push(format!(
                "{}:{line} .{name}() is in code no indexed build compiles, and the checked crate defines {definition}, so the call tier cannot rule out an edge; compile the call in an indexed build",
                file.display()
            ));
        }
        let key = (file.display().to_string(), name.clone());
        match listed.iter().position(|call| *call == key) {
            Some(at) => {
                listed.remove(at);
            }
            None => unlisted.push_str(&format!("\n    {}:{line} .{name}()", file.display())),
        }
    }
    if !unlisted.is_empty() {
        problems.push(format!(
            "uncovered does not list these method calls, which no indexed build compiles:{unlisted}"
        ));
    }
    if !listed.is_empty() {
        let listing: String = listed
            .iter()
            .map(|(file, method)| format!("\n    {file} .{method}()"))
            .collect();
        problems.push(format!(
            "uncovered lists calls that every indexed build misses no longer; remove them:{listing}"
        ));
    }
    let references: Vec<Reference> = found.into_values().collect();
    let summary = format!(
        "call tier: {}; {} references between target crates; {uncovered} method calls in code no indexed build compiles",
        summaries.join("; "),
        references.len(),
    );
    if !problems.is_empty() {
        bail!("{summary}\n{}", problems.join("\n"));
    }
    Ok(CallTier {
        references,
        summary,
    })
}
