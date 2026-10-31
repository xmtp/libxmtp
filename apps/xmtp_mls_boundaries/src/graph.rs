//! The `path` tier: the module tree of one crate, read with `syn` from its
//! `lib.rs`, and every `use` item and `crate::`, `super::` or `self::` path
//! in it, macro bodies included, resolved to the module or item it names.
//!
//! A module whose `cfg` requires `test` and that holds a test function is a
//! test suite and is not read. Every other item is, `cfg(test)` helpers and
//! test-hook modules included. A name a `use` binds, at module or block
//! level, is followed when a path through it reaches another target crate.

use crate::config::Config;
use anyhow::{Context, Result, bail};
use proc_macro2::{TokenStream, TokenTree};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use syn::visit::Visit;

pub type ModPath = Vec<String>;

/// One reference from code mapped to `from` to an item mapped to `to`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Reference {
    pub file: PathBuf,
    pub from: String,
    pub to: String,
    pub path: String,
}

#[derive(Default)]
struct ModuleInfo {
    defined: BTreeSet<String>,
    /// Imported name to the path it imports, relative to this module.
    aliases: BTreeMap<String, Vec<String>>,
    /// Glob imports, relative to this module.
    globs: Vec<Vec<String>>,
}

/// The items one module's files declare, kept for the reference pass.
struct Unit {
    module: ModPath,
    file: PathBuf,
    items: Vec<syn::Item>,
    /// The builds the module is compiled in.
    builds: BTreeSet<Build>,
}

pub struct Crate {
    modules: BTreeMap<ModPath, ModuleInfo>,
    units: Vec<Unit>,
    /// The 1-based line ranges of the test-only inline modules in each file.
    test_lines: BTreeMap<PathBuf, Vec<std::ops::RangeInclusive<usize>>>,
}

/// Whether a `cfg` predicate holds only when `test` does.
fn implies_test(meta: &syn::Meta) -> bool {
    match meta {
        syn::Meta::Path(path) => path.is_ident("test"),
        syn::Meta::List(list) => {
            let nested = list
                .parse_args_with(
                    syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
                )
                .unwrap_or_default();
            if list.path.is_ident("all") {
                nested.iter().any(implies_test)
            } else if list.path.is_ident("any") {
                !nested.is_empty() && nested.iter().all(implies_test)
            } else {
                false
            }
        }
        syn::Meta::NameValue(_) => false,
    }
}

fn is_test_only(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("cfg")
            && attr
                .parse_args::<syn::Meta>()
                .is_ok_and(|meta| implies_test(&meta))
    })
}

/// A build the call tier indexes, with every feature on. Code is checked in
/// each build it is compiled in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Build {
    /// A native target with `test` on.
    NativeTest,
    /// `wasm32-unknown-unknown` with `test` off.
    WasmProduction,
}

impl Build {
    pub const ALL: [Build; 2] = [Build::NativeTest, Build::WasmProduction];

    /// Whether a `cfg` predicate holds in this build. A predicate it does not
    /// know holds, so the code under it stays counted.
    fn holds(self, meta: &syn::Meta) -> bool {
        let wasm = self == Build::WasmProduction;
        match meta {
            syn::Meta::Path(path) => {
                if path.is_ident("test") {
                    !wasm
                } else {
                    !(path.is_ident("doc") || path.is_ident("docsrs"))
                }
            }
            syn::Meta::NameValue(value) => {
                let syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(text),
                    ..
                }) = &value.value
                else {
                    return true;
                };
                if value.path.is_ident("target_arch") {
                    (text.value() == "wasm32") == wasm
                } else if value.path.is_ident("target_family") && text.value() == "wasm" {
                    wasm
                } else {
                    true
                }
            }
            syn::Meta::List(list) => {
                let nested = list
                    .parse_args_with(
                        syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
                    )
                    .unwrap_or_default();
                if list.path.is_ident("all") {
                    nested.iter().all(|meta| self.holds(meta))
                } else if list.path.is_ident("any") {
                    nested.iter().any(|meta| self.holds(meta))
                } else if list.path.is_ident("not") && nested.len() == 1 {
                    !self.holds(&nested[0])
                } else {
                    true
                }
            }
        }
    }

    /// Whether every `cfg` in `attrs` holds in this build.
    fn holds_for(self, attrs: &[syn::Attribute]) -> bool {
        attrs
            .iter()
            .filter(|attr| attr.path().is_ident("cfg"))
            .all(|attr| {
                attr.parse_args::<syn::Meta>()
                    .map_or(true, |meta| self.holds(&meta))
            })
    }
}

/// Whether an attribute marks a test function.
fn is_test_attr(attr: &syn::Attribute) -> bool {
    attr.path().segments.last().is_some_and(|segment| {
        let name = segment.ident.to_string();
        name.ends_with("test") || name == "rstest" || name == "test_case"
    })
}

fn item_attrs(item: &syn::Item) -> &[syn::Attribute] {
    use syn::Item::*;
    match item {
        Const(item) => &item.attrs,
        Enum(item) => &item.attrs,
        ExternCrate(item) => &item.attrs,
        Fn(item) => &item.attrs,
        ForeignMod(item) => &item.attrs,
        Impl(item) => &item.attrs,
        Macro(item) => &item.attrs,
        Mod(item) => &item.attrs,
        Static(item) => &item.attrs,
        Struct(item) => &item.attrs,
        Trait(item) => &item.attrs,
        TraitAlias(item) => &item.attrs,
        Type(item) => &item.attrs,
        Union(item) => &item.attrs,
        Use(item) => &item.attrs,
        _ => &[],
    }
}

fn item_attrs_mut(item: &mut syn::Item) -> Option<&mut Vec<syn::Attribute>> {
    use syn::Item::*;
    Some(match item {
        Const(item) => &mut item.attrs,
        Enum(item) => &mut item.attrs,
        ExternCrate(item) => &mut item.attrs,
        Fn(item) => &mut item.attrs,
        ForeignMod(item) => &mut item.attrs,
        Impl(item) => &mut item.attrs,
        Macro(item) => &mut item.attrs,
        Mod(item) => &mut item.attrs,
        Static(item) => &mut item.attrs,
        Struct(item) => &mut item.attrs,
        Trait(item) => &mut item.attrs,
        TraitAlias(item) => &mut item.attrs,
        Type(item) => &mut item.attrs,
        Union(item) => &mut item.attrs,
        Use(item) => &mut item.attrs,
        _ => return None,
    })
}

/// The `cfg` that one of xmtp_common's item macros puts on what it holds.
fn macro_cfg(path: &syn::Path) -> Option<syn::Attribute> {
    let name = path.segments.last()?.ident.to_string();
    Some(match name.as_str() {
        "if_wasm" => syn::parse_quote!(#[cfg(all(target_family = "wasm", target_os = "unknown"))]),
        "if_native" => {
            syn::parse_quote!(#[cfg(not(all(target_family = "wasm", target_os = "unknown")))])
        }
        "if_only_test" => syn::parse_quote!(#[cfg(test)]),
        "if_test" => syn::parse_quote!(#[cfg(any(test, feature = "test-utils"))]),
        "if_not_test" => syn::parse_quote!(#[cfg(not(any(test, feature = "test-utils")))]),
        "if_dev" => syn::parse_quote!(#[cfg(feature = "dev")]),
        "if_local" => syn::parse_quote!(#[cfg(not(feature = "dev"))]),
        _ => return None,
    })
}

/// Macros that never evaluate their arguments.
const NON_EVALUATING: [&str; 8] = [
    "macro_rules",
    "stringify",
    "concat",
    "include_str",
    "include_bytes",
    "env",
    "option_env",
    "compile_error",
];

/// Whether `name`, raw or not, is one of `NON_EVALUATING`.
fn is_non_evaluating(name: &proc_macro2::Ident) -> bool {
    use syn::ext::IdentExt;
    let name = name.unraw();
    NON_EVALUATING.iter().any(|macro_name| name == macro_name)
}

/// Whether `tokens[index]` holds the arguments of a call of a macro that
/// never evaluates them.
fn is_unevaluated_arguments(tokens: &[TokenTree], index: usize) -> bool {
    index >= 2
        && matches!(&tokens[index - 1], TokenTree::Punct(p) if p.as_char() == '!')
        && matches!(&tokens[index - 2], TokenTree::Ident(name) if is_non_evaluating(name))
}

/// `items` with each of xmtp_common's item macros replaced by the items it
/// holds, under its `cfg` and the macro's own attributes.
fn expand_item_macros(items: Vec<syn::Item>) -> Vec<syn::Item> {
    use syn::parse::Parser;
    let parse_items = |input: syn::parse::ParseStream| {
        let mut items = vec![];
        while !input.is_empty() {
            items.push(input.parse::<syn::Item>()?);
        }
        Ok(items)
    };
    let mut expanded = vec![];
    for item in items {
        if let syn::Item::Macro(mac) = &item
            && let Some(cfg) = macro_cfg(&mac.mac.path)
            && let Ok(inner) = parse_items.parse2(mac.mac.tokens.clone())
        {
            for mut inner in expand_item_macros(inner) {
                if let Some(attrs) = item_attrs_mut(&mut inner) {
                    let mut added = mac.attrs.clone();
                    added.push(cfg.clone());
                    attrs.splice(0..0, added);
                }
                expanded.push(inner);
            }
            continue;
        }
        expanded.push(item);
    }
    expanded
}

fn expr_attrs(expr: &syn::Expr) -> &[syn::Attribute] {
    use syn::Expr::*;
    match expr {
        Array(e) => &e.attrs,
        Assign(e) => &e.attrs,
        Async(e) => &e.attrs,
        Await(e) => &e.attrs,
        Binary(e) => &e.attrs,
        Block(e) => &e.attrs,
        Break(e) => &e.attrs,
        Call(e) => &e.attrs,
        Cast(e) => &e.attrs,
        Closure(e) => &e.attrs,
        Const(e) => &e.attrs,
        Continue(e) => &e.attrs,
        Field(e) => &e.attrs,
        ForLoop(e) => &e.attrs,
        Group(e) => &e.attrs,
        If(e) => &e.attrs,
        Index(e) => &e.attrs,
        Infer(e) => &e.attrs,
        Let(e) => &e.attrs,
        Lit(e) => &e.attrs,
        Loop(e) => &e.attrs,
        Macro(e) => &e.attrs,
        Match(e) => &e.attrs,
        MethodCall(e) => &e.attrs,
        Paren(e) => &e.attrs,
        Path(e) => &e.attrs,
        Range(e) => &e.attrs,
        RawAddr(e) => &e.attrs,
        Reference(e) => &e.attrs,
        Repeat(e) => &e.attrs,
        Return(e) => &e.attrs,
        Struct(e) => &e.attrs,
        Try(e) => &e.attrs,
        TryBlock(e) => &e.attrs,
        Tuple(e) => &e.attrs,
        Unary(e) => &e.attrs,
        Unsafe(e) => &e.attrs,
        While(e) => &e.attrs,
        Yield(e) => &e.attrs,
        _ => &[],
    }
}

/// Method-call counts in one file by 1-based line, 0-based character column
/// and method name.
pub type FileCalls = BTreeMap<(usize, usize, String), usize>;

/// Method-call expressions by build and file.
#[derive(Default)]
pub struct MethodCalls {
    pub active: BTreeMap<Build, BTreeMap<PathBuf, FileCalls>>,
    /// Calls in code that no indexed build compiles: file, 1-based line and
    /// method name.
    pub uncovered: Vec<(PathBuf, usize, String)>,
}

/// Whether the tokens after a method name call it: `(..)`, or a turbofish
/// `::<..>` and then `(..)`.
fn is_call(after: &[TokenTree]) -> bool {
    let is = |index: usize, char: char| matches!(after.get(index), Some(TokenTree::Punct(p)) if p.as_char() == char);
    let mut index = 0;
    if is(0, ':') && is(1, ':') && is(2, '<') {
        let mut depth = 0usize;
        for (at, token) in after.iter().enumerate().skip(2) {
            match token {
                TokenTree::Punct(p) if p.as_char() == '<' => depth += 1,
                TokenTree::Punct(p) if p.as_char() == '>' => {
                    depth -= 1;
                    if depth == 0 {
                        index = at + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        if depth != 0 {
            return false;
        }
    }
    matches!(after.get(index), Some(TokenTree::Group(group)) if group.delimiter() == proc_macro2::Delimiter::Parenthesis)
}

fn parse(file: &Path) -> Result<Vec<syn::Item>> {
    let text =
        std::fs::read_to_string(file).with_context(|| format!("reading {}", file.display()))?;
    Ok(expand_item_macros(
        syn::parse_file(&text)
            .with_context(|| format!("parsing {}", file.display()))?
            .items,
    ))
}

/// The items of module `item`, declared in `file`, with the file they are
/// in and the directory of their child module files.
fn module_items(
    item: &syn::ItemMod,
    file: &Path,
    child_dir: &Path,
) -> Result<(Vec<syn::Item>, PathBuf, PathBuf)> {
    let name = item.ident.to_string();
    if let Some((_, content)) = &item.content {
        return Ok((
            expand_item_macros(content.clone()),
            file.to_path_buf(),
            child_dir.join(&name),
        ));
    }
    let source = match path_attr(&item.attrs) {
        Some(path) => file
            .parent()
            .context("a module file has no directory")?
            .join(path),
        None => {
            let flat = child_dir.join(format!("{name}.rs"));
            let nested = child_dir.join(&name).join("mod.rs");
            match (flat.exists(), nested.exists()) {
                (true, false) => flat,
                (false, true) => nested,
                (true, true) => bail!(
                    "module {name} has both {} and {}",
                    flat.display(),
                    nested.display()
                ),
                (false, false) => bail!("module {name} has no file next to {}", file.display()),
            }
        }
    };
    let parent = source
        .parent()
        .context("a module file has no directory")?
        .to_path_buf();
    let grandchildren = if source.file_name().is_some_and(|f| f == "mod.rs") {
        parent
    } else {
        parent.join(source.file_stem().context("a module file has no name")?)
    };
    Ok((parse(&source)?, source, grandchildren))
}

/// Whether a module holds a test function, in its own items or a child
/// module's.
fn has_tests(items: &[syn::Item], file: &Path, child_dir: &Path) -> Result<bool> {
    for item in items {
        match item {
            syn::Item::Fn(function) if function.attrs.iter().any(is_test_attr) => return Ok(true),
            syn::Item::Mod(module) => {
                let (children, source, grandchildren) = module_items(module, file, child_dir)?;
                if has_tests(&children, &source, &grandchildren)? {
                    return Ok(true);
                }
            }
            _ => {}
        }
    }
    Ok(false)
}

fn path_attr(attrs: &[syn::Attribute]) -> Option<String> {
    attrs.iter().find_map(|attr| match &attr.meta {
        syn::Meta::NameValue(value) if value.path.is_ident("path") => match &value.value {
            syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(path),
                ..
            }) => Some(path.value()),
            _ => None,
        },
        _ => None,
    })
}

/// A flattened `use` leaf: the imported path, and the name it binds or
/// `None` for a glob.
fn use_leaves(
    tree: &syn::UseTree,
    prefix: &mut Vec<String>,
    out: &mut Vec<(Vec<String>, Option<String>)>,
) {
    match tree {
        syn::UseTree::Path(path) => {
            prefix.push(path.ident.to_string());
            use_leaves(&path.tree, prefix, out);
            prefix.pop();
        }
        syn::UseTree::Name(name) => {
            let ident = name.ident.to_string();
            if ident == "self" {
                let bound = prefix.last().cloned();
                out.push((prefix.clone(), bound));
            } else {
                let mut path = prefix.clone();
                path.push(ident.clone());
                out.push((path, Some(ident)));
            }
        }
        syn::UseTree::Rename(rename) => {
            let ident = rename.ident.to_string();
            let mut path = prefix.clone();
            if ident != "self" {
                path.push(ident);
            }
            let bound = rename.rename.to_string();
            out.push((path, (bound != "_").then_some(bound)));
        }
        syn::UseTree::Glob(_) => out.push((prefix.clone(), None)),
        syn::UseTree::Group(group) => {
            for tree in &group.items {
                use_leaves(tree, prefix, out);
            }
        }
    }
}

fn item_ident(item: &syn::Item) -> Option<String> {
    match item {
        syn::Item::Const(item) => Some(item.ident.to_string()),
        syn::Item::Enum(item) => Some(item.ident.to_string()),
        syn::Item::Fn(item) => Some(item.sig.ident.to_string()),
        syn::Item::Macro(item) => item.ident.as_ref().map(ToString::to_string),
        syn::Item::Static(item) => Some(item.ident.to_string()),
        syn::Item::Struct(item) => Some(item.ident.to_string()),
        syn::Item::Trait(item) => Some(item.ident.to_string()),
        syn::Item::TraitAlias(item) => Some(item.ident.to_string()),
        syn::Item::Type(item) => Some(item.ident.to_string()),
        syn::Item::Union(item) => Some(item.ident.to_string()),
        _ => None,
    }
}

/// The last segment of an impl block's self type, the name its methods are
/// mapped under.
fn self_type_name(ty: &syn::Type) -> Option<String> {
    match ty {
        syn::Type::Path(path) => path
            .path
            .segments
            .last()
            .map(|segment| segment.ident.to_string()),
        syn::Type::Reference(reference) => self_type_name(&reference.elem),
        _ => None,
    }
}

/// How sure a resolution is: `Certain` when the name was found, `Guess`
/// when the path continued past what the reader can see.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Found {
    Certain,
    Guess,
}

const MAX_DEPTH: usize = 16;

impl Crate {
    /// Read the crate whose root file is `lib`.
    pub fn load(lib: &Path) -> Result<Self> {
        let mut krate = Crate {
            modules: BTreeMap::new(),
            units: vec![],
            test_lines: BTreeMap::new(),
        };
        let dir = lib.parent().context("the crate root has no directory")?;
        krate.load_items(parse(lib)?, vec![], lib, dir, Build::ALL.into())?;
        /// Code that neither tier reads as written, by line: modules declared
        /// in a block or in macro arguments, and local macros named like one
        /// that never evaluates its arguments.
        struct HiddenModules(Vec<(usize, String)>);
        impl HiddenModules {
            fn shadows(&mut self, name: &proc_macro2::Ident) {
                if is_non_evaluating(name) {
                    self.0.push((
                        name.span().start().line,
                        format!("defines a macro named {name}, which the checker reads as never evaluating its arguments; rename it"),
                    ));
                }
            }
            fn scan(&mut self, tokens: TokenStream) {
                let tokens: Vec<TokenTree> = tokens.into_iter().collect();
                for (index, token) in tokens.iter().enumerate() {
                    match token {
                        TokenTree::Group(_) if is_unevaluated_arguments(&tokens, index) => {}
                        TokenTree::Group(group) => self.scan(group.stream()),
                        TokenTree::Ident(keyword) if keyword == "macro_rules" => {
                            if let (Some(TokenTree::Punct(bang)), Some(TokenTree::Ident(name))) =
                                (tokens.get(index + 1), tokens.get(index + 2))
                                && bang.as_char() == '!'
                            {
                                self.shadows(name);
                            }
                        }
                        TokenTree::Ident(keyword) if keyword == "mod" => {
                            let declares =
                                matches!(tokens.get(index + 1), Some(TokenTree::Ident(_)))
                                    && match tokens.get(index + 2) {
                                        Some(TokenTree::Punct(p)) => p.as_char() == ';',
                                        Some(TokenTree::Group(g)) => {
                                            g.delimiter() == proc_macro2::Delimiter::Brace
                                        }
                                        _ => false,
                                    };
                            if declares {
                                self.0.push((
                                    keyword.span().start().line,
                                    "declares a module in macro arguments, which neither tier reads; move it to module level".to_string(),
                                ));
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        impl<'ast> Visit<'ast> for HiddenModules {
            fn visit_item_mod(&mut self, _: &'ast syn::ItemMod) {}
            fn visit_block(&mut self, block: &'ast syn::Block) {
                for stmt in &block.stmts {
                    if let syn::Stmt::Item(syn::Item::Mod(module)) = stmt {
                        self.0.push((
                            module.ident.span().start().line,
                            "declares a module inside a block, which neither tier reads; move it to module level".to_string(),
                        ));
                    }
                }
                syn::visit::visit_block(self, block);
            }
            fn visit_item_macro(&mut self, item: &'ast syn::ItemMacro) {
                if item.mac.path.is_ident("macro_rules")
                    && let Some(name) = &item.ident
                {
                    self.shadows(name);
                }
                syn::visit::visit_item_macro(self, item);
            }
            fn visit_item_extern_crate(&mut self, item: &'ast syn::ItemExternCrate) {
                use syn::ext::IdentExt;
                if let Some((_, alias)) = &item.rename
                    && ["std", "core", "alloc"]
                        .iter()
                        .any(|root| alias.unraw() == root)
                    && item.ident.unraw() != alias.unraw()
                {
                    self.0.push((
                        alias.span().start().line,
                        format!("imports the crate {} as {alias}, which the checker reads as the standard library; rename it", item.ident),
                    ));
                }
            }
            fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
                let mut leaves = vec![];
                use_leaves(&item.tree, &mut vec![], &mut leaves);
                for (path, bound) in leaves {
                    let Some(bound) = bound else { continue };
                    let name = bound.trim_start_matches("r#");
                    let path: Vec<&str> = path
                        .iter()
                        .map(|segment| segment.trim_start_matches("r#"))
                        .collect();
                    let builtin = matches!(path.as_slice(), [root, .., last]
                        if ["std", "core", "alloc"].contains(root) && *last == name);
                    if ["std", "core", "alloc"].contains(&name) && path != [name] {
                        self.0.push((
                            item.use_token.span.start().line,
                            format!("imports {} as {name}, which the checker reads as the standard library; rename it", path.join("::")),
                        ));
                    }
                    if !builtin && NON_EVALUATING.contains(&name) {
                        self.0.push((
                            item.use_token.span.start().line,
                            format!("imports {} as {name}, which the checker reads as a macro that never evaluates its arguments; rename it", path.join("::")),
                        ));
                    }
                }
                syn::visit::visit_item_use(self, item);
            }
            fn visit_macro(&mut self, mac: &'ast syn::Macro) {
                let evaluates = mac
                    .path
                    .segments
                    .last()
                    .is_none_or(|segment| !is_non_evaluating(&segment.ident));
                if evaluates {
                    self.scan(mac.tokens.clone());
                }
            }
        }
        for unit in &krate.units {
            let mut found = HiddenModules(vec![]);
            for item in &unit.items {
                found.visit_item(item);
            }
            if let Some((line, problem)) = found.0.first() {
                bail!("{}:{line} {problem}", unit.file.display());
            }
        }
        Ok(krate)
    }

    /// Load a module's items. A module whose `cfg` requires `test` and that
    /// holds a test function is a test suite and is not read; other
    /// test-only modules, such as test hooks, are.
    fn load_items(
        &mut self,
        items: Vec<syn::Item>,
        module: ModPath,
        file: &Path,
        child_dir: &Path,
        builds: BTreeSet<Build>,
    ) -> Result<()> {
        self.modules.entry(module.clone()).or_default();
        for item in &items {
            if let Some(name) = item_ident(item) {
                self.info(&module).defined.insert(name);
            }
            match item {
                syn::Item::Use(item) => {
                    let mut leaves = vec![];
                    use_leaves(&item.tree, &mut vec![], &mut leaves);
                    let info = self.info(&module);
                    for (path, bound) in leaves {
                        match bound {
                            Some(name) => {
                                info.aliases.insert(name, path);
                            }
                            None => info.globs.push(path),
                        }
                    }
                }
                syn::Item::Mod(item) => {
                    let (children, source, grandchildren) = module_items(item, file, child_dir)?;
                    if is_test_only(&item.attrs) && has_tests(&children, &source, &grandchildren)? {
                        if let Some((brace, _)) = &item.content {
                            self.test_lines.entry(file.to_path_buf()).or_default().push(
                                item.mod_token.span.start().line..=brace.span.close().end().line,
                            );
                        }
                        continue;
                    }
                    let mut child = module.clone();
                    child.push(item.ident.to_string());
                    let builds = builds
                        .iter()
                        .copied()
                        .filter(|build| build.holds_for(&item.attrs))
                        .collect();
                    self.load_items(children, child, &source, &grandchildren, builds)?;
                }
                _ => {}
            }
        }
        self.units.push(Unit {
            module,
            file: file.to_path_buf(),
            items,
            builds,
        });
        Ok(())
    }

    fn info(&mut self, module: &ModPath) -> &mut ModuleInfo {
        self.modules.entry(module.clone()).or_default()
    }

    /// The module a file's top-level items belong to.
    pub fn module_of(&self, file: &Path) -> Option<&ModPath> {
        self.units
            .iter()
            .find(|unit| unit.file == file)
            .map(|unit| &unit.module)
    }

    /// The method calls, by the builds that compile them, with the position
    /// of each method name: method-call expressions, and in the arguments of
    /// macros that may evaluate them every `.name(` or `.name::<..>(`.
    pub fn method_calls(&self) -> MethodCalls {
        struct Calls<'a> {
            calls: &'a mut MethodCalls,
            file: PathBuf,
            builds: BTreeSet<Build>,
        }
        impl Calls<'_> {
            fn add(&mut self, method: &proc_macro2::Ident) {
                let start = method.span().start();
                if self.builds.is_empty() {
                    self.calls
                        .uncovered
                        .push((self.file.clone(), start.line, method.to_string()));
                }
                let key = (start.line, start.column, method.to_string());
                for build in &self.builds {
                    *self
                        .calls
                        .active
                        .entry(*build)
                        .or_default()
                        .entry(self.file.clone())
                        .or_default()
                        .entry(key.clone())
                        .or_default() += 1;
                }
            }
            fn only(&mut self, build: Build, visit: impl FnOnce(&mut Self)) {
                let was = self.builds.clone();
                self.builds.retain(|kept| *kept == build);
                visit(self);
                self.builds = was;
            }
            /// Read macro arguments as statements when they parse as such,
            /// so their `cfg` attributes apply; otherwise scan their tokens.
            fn read_tokens(&mut self, tokens: TokenStream) {
                use syn::parse::Parser;
                match syn::Block::parse_within.parse2(tokens.clone()) {
                    Ok(stmts) => {
                        for stmt in &stmts {
                            self.visit_stmt(stmt);
                        }
                    }
                    Err(_) => self.scan_tokens(tokens),
                }
            }
            fn scan_tokens(&mut self, tokens: TokenStream) {
                let tokens: Vec<TokenTree> = tokens.into_iter().collect();
                for (index, token) in tokens.iter().enumerate() {
                    match token {
                        TokenTree::Group(_) if is_unevaluated_arguments(&tokens, index) => {}
                        TokenTree::Group(group)
                            if group.delimiter() == proc_macro2::Delimiter::Brace =>
                        {
                            self.read_tokens(group.stream())
                        }
                        TokenTree::Group(group) => self.scan_tokens(group.stream()),
                        TokenTree::Punct(dot) if dot.as_char() == '.' => {
                            let after_range = index > 0
                                && matches!(&tokens[index - 1], TokenTree::Punct(p) if p.as_char() == '.');
                            if let (false, Some(TokenTree::Ident(method))) =
                                (after_range, tokens.get(index + 1))
                                && method != "await"
                                && is_call(&tokens[index + 2..])
                            {
                                self.add(method);
                            }
                        }
                        _ => {}
                    }
                }
            }
            fn guarded(&mut self, attrs: &[syn::Attribute], visit: impl FnOnce(&mut Self)) {
                let was = self.builds.clone();
                self.builds.retain(|build| build.holds_for(attrs));
                visit(self);
                self.builds = was;
            }
        }
        impl<'ast> Visit<'ast> for Calls<'_> {
            fn visit_item_mod(&mut self, _: &'ast syn::ItemMod) {}
            fn visit_item(&mut self, item: &'ast syn::Item) {
                self.guarded(item_attrs(item), |this| syn::visit::visit_item(this, item));
            }
            fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
                self.guarded(&item.attrs, |this| {
                    syn::visit::visit_impl_item_fn(this, item)
                });
            }
            fn visit_impl_item_const(&mut self, item: &'ast syn::ImplItemConst) {
                self.guarded(&item.attrs, |this| {
                    syn::visit::visit_impl_item_const(this, item)
                });
            }
            fn visit_trait_item_fn(&mut self, item: &'ast syn::TraitItemFn) {
                self.guarded(&item.attrs, |this| {
                    syn::visit::visit_trait_item_fn(this, item)
                });
            }
            fn visit_local(&mut self, local: &'ast syn::Local) {
                self.guarded(&local.attrs, |this| syn::visit::visit_local(this, local));
            }
            fn visit_arm(&mut self, arm: &'ast syn::Arm) {
                self.guarded(&arm.attrs, |this| syn::visit::visit_arm(this, arm));
            }
            fn visit_field_value(&mut self, field: &'ast syn::FieldValue) {
                self.guarded(&field.attrs, |this| {
                    syn::visit::visit_field_value(this, field)
                });
            }
            fn visit_expr(&mut self, expr: &'ast syn::Expr) {
                self.guarded(expr_attrs(expr), |this| syn::visit::visit_expr(this, expr));
            }
            fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
                self.add(&call.method);
                syn::visit::visit_expr_method_call(self, call);
            }
            fn visit_macro(&mut self, mac: &'ast syn::Macro) {
                use syn::ext::IdentExt;
                use syn::parse::Parser;
                let name = mac
                    .path
                    .segments
                    .last()
                    .map(|segment| segment.ident.unraw().to_string())
                    .unwrap_or_default();
                let platform = |name: &str| match name {
                    "wasm" => Some(Build::WasmProduction),
                    "native" => Some(Build::NativeTest),
                    _ => None,
                };
                if let Some(cfg) = macro_cfg(&mac.path) {
                    let tokens = mac.tokens.clone().into_iter().skip_while(
                        |token| matches!(token, TokenTree::Punct(p) if p.as_char() == '@'),
                    );
                    self.guarded(&[cfg], |this| this.read_tokens(tokens.collect()));
                    return;
                }
                match name.as_str() {
                    name if NON_EVALUATING.contains(&name) => {}
                    "wasm_or_native" => {
                        let arms = |input: syn::parse::ParseStream| {
                            let mut arms = vec![];
                            while !input.is_empty() {
                                let platform: syn::Ident = input.parse()?;
                                input.parse::<syn::Token![=>]>()?;
                                let block: syn::Block = input.parse()?;
                                input.parse::<Option<syn::Token![,]>>()?;
                                arms.push((platform, block));
                            }
                            Ok(arms)
                        };
                        match arms.parse2(mac.tokens.clone()) {
                            Ok(arms) => {
                                for (arm, block) in &arms {
                                    match platform(&arm.to_string()) {
                                        Some(build) => {
                                            self.only(build, |this| this.visit_block(block))
                                        }
                                        None => self.visit_block(block),
                                    }
                                }
                            }
                            Err(_) => self.scan_tokens(mac.tokens.clone()),
                        }
                    }
                    _ => self.read_tokens(mac.tokens.clone()),
                }
            }
        }
        let mut calls = MethodCalls::default();
        for unit in &self.units {
            let mut visitor = Calls {
                calls: &mut calls,
                file: unit.file.clone(),
                builds: unit.builds.clone(),
            };
            for item in &unit.items {
                visitor.visit_item(item);
            }
        }
        calls
    }

    /// Every method name that an impl or trait of a read module defines, with
    /// the path of one such definition.
    pub fn defined_methods(&self) -> BTreeMap<String, String> {
        struct Methods<'a> {
            module: &'a ModPath,
            found: &'a mut BTreeMap<String, String>,
        }
        impl Methods<'_> {
            fn add(&mut self, owner: String, method: &syn::Ident) {
                let mut path = self.module.clone();
                path.extend([owner, method.to_string()]);
                self.found
                    .entry(method.to_string())
                    .or_insert_with(|| path.join("::"));
            }
        }
        impl<'ast> Visit<'ast> for Methods<'_> {
            fn visit_item_mod(&mut self, _: &'ast syn::ItemMod) {}
            fn visit_item_impl(&mut self, item: &'ast syn::ItemImpl) {
                let owner = match &*item.self_ty {
                    syn::Type::Path(path) => path.path.segments.last().map(|s| s.ident.to_string()),
                    _ => None,
                }
                .unwrap_or_else(|| "_".to_string());
                for inner in &item.items {
                    if let syn::ImplItem::Fn(method) = inner {
                        self.add(owner.clone(), &method.sig.ident);
                    }
                }
                syn::visit::visit_item_impl(self, item);
            }
            fn visit_item_trait(&mut self, item: &'ast syn::ItemTrait) {
                for inner in &item.items {
                    if let syn::TraitItem::Fn(method) = inner {
                        self.add(item.ident.to_string(), &method.sig.ident);
                    }
                }
                syn::visit::visit_item_trait(self, item);
            }
        }
        let mut found = BTreeMap::new();
        for unit in &self.units {
            let mut methods = Methods {
                module: &unit.module,
                found: &mut found,
            };
            for item in &unit.items {
                methods.visit_item(item);
            }
        }
        found
    }

    /// Whether 1-based `line` of `file` lies in a test-only inline module.
    pub fn in_test_module(&self, file: &Path, line: usize) -> bool {
        self.test_lines
            .get(file)
            .is_some_and(|ranges| ranges.iter().any(|range| range.contains(&line)))
    }

    /// The files of the modules that `build` compiles.
    pub fn files_in(&self, build: Build) -> BTreeSet<&Path> {
        self.units
            .iter()
            .filter(|unit| unit.builds.contains(&build))
            .map(|unit| unit.file.as_path())
            .collect()
    }

    /// `segments` written in `from`, made absolute, or `None` when they name
    /// another crate.
    fn absolute(&self, from: &[String], segments: &[String]) -> Option<Vec<String>> {
        let (mut base, rest) = match segments.first()?.as_str() {
            "crate" | "$crate" => (vec![], &segments[1..]),
            "self" => (from.to_vec(), &segments[1..]),
            "super" => {
                let mut base = from.to_vec();
                let mut rest = segments;
                while rest.first().is_some_and(|segment| segment == "super") {
                    base.pop()?;
                    rest = &rest[1..];
                }
                (base, rest)
            }
            first => {
                let info = self.modules.get(from)?;
                let mut child = from.to_vec();
                child.push(first.to_string());
                let local = self.modules.contains_key(&child)
                    || info.defined.contains(first)
                    || info.aliases.contains_key(first);
                if !local {
                    return None;
                }
                (from.to_vec(), segments)
            }
        };
        base.extend(rest.iter().cloned());
        Some(base)
    }

    /// The module and the item path inside it that `segments`, written in
    /// `from`, name; `None` for another crate.
    fn resolve(
        &self,
        from: &[String],
        segments: &[String],
        depth: usize,
    ) -> Option<(ModPath, Vec<String>, Found)> {
        if depth > MAX_DEPTH {
            return None;
        }
        let path = self.absolute(from, segments)?;
        let mut module: ModPath = vec![];
        for (index, name) in path.iter().enumerate() {
            let mut child = module.clone();
            child.push(name.clone());
            if self.modules.contains_key(&child) {
                module = child;
                continue;
            }
            let info = &self.modules[&module];
            let rest = path[index..].to_vec();
            if info.defined.contains(name) {
                return Some((module, rest, Found::Certain));
            }
            if let Some(target) = info.aliases.get(name) {
                let mut segments = target.clone();
                segments.extend(path[index + 1..].iter().cloned());
                return self.resolve(&module, &segments, depth + 1);
            }
            let mut external = false;
            for glob in &info.globs {
                let mut segments = glob.clone();
                segments.extend(rest.iter().cloned());
                external |= self.absolute(&module, glob).is_none();
                if let Some(found @ (_, _, Found::Certain)) =
                    self.resolve(&module, &segments, depth + 1)
                {
                    return Some(found);
                }
            }
            if external {
                return None;
            }
            return Some((module, rest, Found::Guess));
        }
        Some((module, vec![], Found::Certain))
    }

    /// Every reference between two different target crates.
    pub fn references(&self, config: &Config, root: &Path) -> Vec<Reference> {
        let mut references = vec![];
        for unit in &self.units {
            let file = unit
                .file
                .strip_prefix(root)
                .unwrap_or(&unit.file)
                .to_path_buf();
            let mut collector = Collector {
                krate: self,
                config,
                module: &unit.module,
                file,
                owner: vec![],
                scopes: vec![],
                references: &mut references,
            };
            for item in &unit.items {
                collector.visit_top(item);
            }
        }
        references.sort();
        references
    }
}

/// The target crate of an item path inside `module`: the longest item
/// override, else the longest module prefix.
pub fn target_of<'a>(config: &'a Config, module: &[String], item: &[String]) -> &'a str {
    for length in (1..=item.len()).rev() {
        let mut key = module.to_vec();
        key.extend(item[..length].iter().cloned());
        if let Some(target) = config.items.get(&key.join("::")) {
            return target;
        }
    }
    for length in (0..=module.len()).rev() {
        if let Some(target) = config.modules.get(&module[..length].join("::")) {
            return target;
        }
    }
    unreachable!("the configuration maps the crate root")
}

struct Collector<'a> {
    krate: &'a Crate,
    config: &'a Config,
    module: &'a ModPath,
    file: PathBuf,
    /// The item, or impl type and method, whose body is being read.
    owner: Vec<String>,
    /// The names that `use` items bind in the enclosing blocks, innermost
    /// last.
    scopes: Vec<BTreeMap<String, Vec<String>>>,
    references: &'a mut Vec<Reference>,
}

impl Collector<'_> {
    fn visit_top(&mut self, item: &syn::Item) {
        match item {
            syn::Item::Mod(_) => {}
            syn::Item::Impl(item) => {
                let name = self_type_name(&item.self_ty);
                self.owner = name.iter().cloned().collect();
                if let Some((_, path, _)) = &item.trait_ {
                    self.visit_path(path);
                }
                self.visit_type(&item.self_ty);
                self.visit_generics(&item.generics);
                for member in &item.items {
                    self.owner = name.iter().cloned().collect();
                    if let syn::ImplItem::Fn(function) = member {
                        self.owner.push(function.sig.ident.to_string());
                    }
                    self.visit_impl_item(member);
                }
            }
            item => {
                self.owner = item_ident(item).into_iter().collect();
                self.visit_item(item);
            }
        }
        self.owner.clear();
    }

    fn record(&mut self, segments: Vec<String>) {
        let Some((module, item, _)) = self.krate.resolve(self.module, &segments, 0) else {
            return;
        };
        let from = target_of(self.config, self.module, &self.owner);
        let to = target_of(self.config, &module, &item);
        if from == to {
            return;
        }
        self.references.push(Reference {
            file: self.file.clone(),
            from: from.to_string(),
            to: to.to_string(),
            path: segments.join("::"),
        });
    }

    /// The path a block-scoped `use` binds to `name`, innermost first.
    fn bound(&self, name: &str) -> Option<&Vec<String>> {
        self.scopes.iter().rev().find_map(|scope| scope.get(name))
    }

    /// Record `full`, a path through an imported name `alias`, when it ends
    /// in another target crate than the import, which is recorded already.
    fn record_beyond(&mut self, alias: &[String], full: Vec<String>) {
        let target = |segments: &[String]| {
            self.krate
                .resolve(self.module, segments, 0)
                .map(|(module, item, _)| target_of(self.config, &module, &item))
        };
        if let (Some(imported), Some(named)) = (target(alias), target(&full))
            && imported != named
        {
            self.record(full);
        }
    }

    fn scan_tokens(&mut self, tokens: TokenStream) {
        let tokens: Vec<TokenTree> = tokens.into_iter().collect();
        let mut index = 0;
        while index < tokens.len() {
            if let TokenTree::Group(group) = &tokens[index] {
                self.scan_tokens(group.stream());
                index += 1;
                continue;
            }
            let start = match &tokens[index] {
                TokenTree::Ident(ident)
                    if ["crate", "super", "self"].contains(&ident.to_string().as_str()) =>
                {
                    Some(ident.to_string())
                }
                _ => None,
            };
            let Some(start) = start else {
                index += 1;
                continue;
            };
            let mut segments = vec![start];
            let mut next = index + 1;
            while let (
                Some(TokenTree::Punct(first)),
                Some(TokenTree::Punct(second)),
                Some(TokenTree::Ident(ident)),
            ) = (tokens.get(next), tokens.get(next + 1), tokens.get(next + 2))
            {
                if first.as_char() != ':' || second.as_char() != ':' {
                    break;
                }
                segments.push(ident.to_string());
                next += 3;
            }
            if segments.len() > 1 {
                self.record(segments);
            }
            index = next;
        }
    }
}

impl<'ast> Visit<'ast> for Collector<'_> {
    fn visit_item_mod(&mut self, _: &'ast syn::ItemMod) {}

    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        let mut leaves = vec![];
        use_leaves(&item.tree, &mut vec![], &mut leaves);
        for (path, _) in leaves {
            self.record(path);
        }
    }

    fn visit_block(&mut self, block: &'ast syn::Block) {
        let mut leaves = vec![];
        for stmt in &block.stmts {
            if let syn::Stmt::Item(syn::Item::Use(item)) = stmt {
                use_leaves(&item.tree, &mut vec![], &mut leaves);
            }
        }
        // A block's `use` items are in scope in the whole block, so an import
        // may go through a name that a later one binds.
        let mut paths: Vec<Vec<String>> = leaves.iter().map(|(path, _)| path.clone()).collect();
        for _ in 0..leaves.len() {
            let next: Vec<Vec<String>> = leaves
                .iter()
                .enumerate()
                .map(|(index, (path, _))| {
                    let first = path.first().cloned().unwrap_or_default();
                    let local = leaves
                        .iter()
                        .zip(&paths)
                        .enumerate()
                        .find(|(binder, ((_, name), _))| {
                            *binder != index && name.as_deref() == Some(first.as_str())
                        })
                        .map(|(_, (_, binding))| binding);
                    match local.or_else(|| self.bound(&first)) {
                        Some(binding) => {
                            let mut full = binding.clone();
                            full.extend(path.iter().skip(1).cloned());
                            full
                        }
                        None => path.clone(),
                    }
                })
                .collect();
            if next == paths {
                break;
            }
            paths = next;
        }
        for path in &paths {
            self.record(path.clone());
        }
        let scope: BTreeMap<String, Vec<String>> = leaves
            .into_iter()
            .zip(paths)
            .filter_map(|((_, name), path)| name.map(|name| (name, path)))
            .collect();
        self.scopes.push(scope);
        for stmt in &block.stmts {
            if !matches!(stmt, syn::Stmt::Item(syn::Item::Use(_))) {
                self.visit_stmt(stmt);
            }
        }
        self.scopes.pop();
    }

    fn visit_path(&mut self, path: &'ast syn::Path) {
        let segments: Vec<String> = path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect();
        if segments.len() > 1 && path.leading_colon.is_none() {
            let first = segments[0].as_str();
            if ["crate", "super", "self"].contains(&first) {
                self.record(segments);
            } else if let Some(binding) = self.bound(first).cloned() {
                let mut full = binding.clone();
                full.extend(segments[1..].iter().cloned());
                self.record_beyond(&binding, full);
            } else if self
                .krate
                .modules
                .get(self.module)
                .is_some_and(|info| info.aliases.contains_key(first))
            {
                self.record_beyond(&segments[..1], segments.clone());
            }
        }
        syn::visit::visit_path(self, path);
    }

    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        self.visit_path(&mac.path);
        self.scan_tokens(mac.tokens.clone());
    }
}
