//! Client statics from `#[sdk_export(client_static)]`.
//!
//! The macro marks an asynchronous free function with `@xmtp-client-static`,
//! and each SDK's Client gets a static member that calls it. The static's
//! name is the function's without a trailing `_with_backend`, in the SDK's
//! casing. It takes the function's parameters in order, with the
//! `BackendSource` parameter moved last. The function itself stays exported.
//!
//! The TypeScript statics sit on the generated `ClientMembers` base in the
//! public projection. The Kotlin and Swift statics go to the generated
//! `ClientForwarding` file, with the parameter and result types that the
//! stock binding declares for the function.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
};

use anyhow::{Context, Result, bail};
use heck::ToLowerCamelCase;
use uniffi_meta::{FieldMetadata, FnMetadata, FnParamMetadata, Metadata, Type};

use crate::{markers, redaction::swift_identifier};

/// The suffix of a function that takes its backend as an argument.
const BACKEND_SUFFIX: &str = "_with_backend";
/// The façade enum that selects a backend.
const BACKEND_SOURCE: &str = "BackendSource";
/// Static names that the Client class cannot take: the host constructors,
/// the class constructor, which TypeScript does not let a static method be
/// named, the properties of every JavaScript function, which a TypeScript
/// class is, and the Swift declarations that `SDKClient.init` and the like
/// would name instead of a static method.
const TAKEN_STATICS: &[(&str, &str)] = &[
    ("arguments", "a property of every JavaScript function"),
    ("build", "a host constructor"),
    ("caller", "a property of every JavaScript function"),
    ("constructor", "the class constructor"),
    ("create", "a host constructor"),
    ("deinit", "a Swift deinitializer"),
    ("init", "a Swift initializer"),
    ("length", "a property of every JavaScript function"),
    ("name", "a property of every JavaScript function"),
    ("prototype", "a property of every JavaScript function"),
    ("subscript", "a Swift subscript"),
];
/// The Swift module of the generated package. A static named like its
/// function (`fetchServerConfiguration`) calls the function through it,
/// because the static's own name would resolve to the static.
const SWIFT_MODULE: &str = "XmtpSdk";
/// The Kotlin package of the generated binding, for the same reason.
pub(crate) const KOTLIN_PACKAGE: &str = "uniffi.xmtp_sdk";
/// The Kotlin runtime wraps each foreign value that a call passes to Rust,
/// so a host failure reaches Rust as the error that its trait declares. Each
/// entry names a type and its wrapper in `runtime/kotlin/SDKForeign.kt`.
const KOTLIN_FOREIGN: &[(&str, &str)] = &[
    (BACKEND_SOURCE, "backend"),
    ("CredentialSource", "credentials"),
    ("LogSink", "logSink"),
    ("Signer", "signer"),
];

/// One Client static.
pub(crate) struct ClientStatic<'a> {
    pub(crate) function: &'a FnMetadata,
    /// The static's name, in lower camel case.
    pub(crate) name: String,
    /// The function's parameters in the static's order.
    pub(crate) parameters: Vec<&'a FnParamMetadata>,
}

/// The Client statics, sorted by name. Generation stops on a marked
/// function that the rule cannot express.
pub(crate) fn client_statics<'a>(items: &[&'a Metadata]) -> Result<Vec<ClientStatic<'a>>> {
    let mut statics = Vec::new();
    let mut names = BTreeSet::new();
    for item in items {
        let Metadata::Func(function) = item else {
            continue;
        };
        let doc = function.docstring.as_deref();
        if !markers::has(doc, markers::CLIENT_STATIC) {
            continue;
        }
        let owner = &function.name;
        if !function.is_async {
            bail!("{owner}: a client static calls an asynchronous function");
        }
        if markers::has(doc, markers::INTERNAL) {
            bail!("{owner}: a client static is public; it cannot be @xmtp-internal");
        }
        let base = owner.strip_suffix(BACKEND_SUFFIX).unwrap_or(owner);
        // The name reaches generated code: only a lowercase ASCII Rust name
        // is an identifier in every SDK.
        if !base.starts_with(|c: char| c.is_ascii_lowercase())
            || !base
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        {
            bail!("{owner}: a client static needs a lowercase ASCII function name");
        }
        let name = base.to_lower_camel_case();
        if let Some((_, taken)) = TAKEN_STATICS.iter().find(|(taken, _)| *taken == name) {
            bail!("{owner}: Client.{name} is {taken}; rename the function");
        }
        if !names.insert(name.clone()) {
            bail!("{owner}: another client static is also Client.{name}");
        }
        let (backends, mut parameters): (Vec<_>, Vec<_>) = function
            .inputs
            .iter()
            .partition(|parameter| is_backend(&parameter.ty));
        if backends.len() > 1 {
            bail!("{owner}: a client static takes at most one BackendSource");
        }
        // The static exists to take the backend, and a required last backend
        // keeps the static's optional parameters those of the function.
        if let Some(backend) = backends.iter().find(|backend| backend.default.is_some()) {
            bail!(
                "{owner}.{}: a client static's BackendSource cannot have a default",
                backend.name
            );
        }
        parameters.extend(backends);
        // A parameter that callers may leave out stays one: the static keeps
        // it among the trailing defaulted parameters. TypeScript cannot leave
        // out a parameter before a required one, such as the moved backend.
        let static_defaults = trailing_defaults(parameters.iter().copied());
        if let Some(lost) = trailing_defaults(function.inputs.iter())
            .difference(&static_defaults)
            .next()
        {
            bail!(
                "{owner}.{lost}: the static takes the BackendSource after this defaulted parameter, so a TypeScript caller could not leave it out; drop the default"
            );
        }
        statics.push(ClientStatic {
            function,
            name,
            parameters,
        });
    }
    statics.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(statics)
}

fn is_backend(ty: &Type) -> bool {
    matches!(ty, Type::Enum { name, .. } if name == BACKEND_SOURCE)
}

/// The names of the trailing parameters that have a default.
fn trailing_defaults<'a>(
    parameters: impl DoubleEndedIterator<Item = &'a FnParamMetadata>,
) -> BTreeSet<&'a str> {
    parameters
        .rev()
        .take_while(|parameter| parameter.default.is_some())
        .map(|parameter| parameter.name.as_str())
        .collect()
}

/// The foreign type that the Kotlin runtime wraps before a call: a backend,
/// or a foreign trait.
fn foreign(ty: &Type) -> Option<&str> {
    match ty {
        Type::Enum { name, .. } if name == BACKEND_SOURCE => Some(name),
        Type::CallbackInterface { name, .. } => Some(name),
        Type::Object { name, imp, .. } if imp.has_callback_interface() => Some(name),
        _ => None,
    }
}

/// The fields of each record and enum, to look inside a parameter's type.
struct Fields<'a>(BTreeMap<&'a str, Vec<&'a FieldMetadata>>);

impl<'a> Fields<'a> {
    fn new(items: &[&'a Metadata]) -> Self {
        let mut fields = BTreeMap::new();
        for item in items {
            match item {
                Metadata::Record(record) => {
                    fields.insert(record.name.as_str(), record.fields.iter().collect());
                }
                Metadata::Enum(value) => {
                    let all = value
                        .variants
                        .iter()
                        .flat_map(|variant| &variant.fields)
                        .collect();
                    fields.insert(value.name.as_str(), all);
                }
                _ => {}
            }
        }
        Self(fields)
    }

    /// The first foreign type inside `ty` that a wrapper of `ty` itself
    /// would not reach: in a container, a record field, or an enum variant.
    fn inner_foreign(&self, ty: &'a Type, seen: &mut BTreeSet<&'a str>) -> Option<&'a str> {
        match ty {
            Type::Optional { inner_type }
            | Type::Sequence { inner_type }
            | Type::Set { inner_type }
            | Type::Box { inner_type } => self.reached(inner_type, seen),
            Type::Map {
                key_type,
                value_type,
            } => self
                .reached(key_type, seen)
                .or_else(|| self.reached(value_type, seen)),
            Type::Custom { builtin, .. } => self.reached(builtin, seen),
            Type::Record { name, .. } | Type::Enum { name, .. } => {
                if !seen.insert(name) {
                    return None;
                }
                self.0
                    .get(name.as_str())?
                    .iter()
                    .find_map(|field| self.reached(&field.ty, seen))
            }
            _ => None,
        }
    }

    /// A foreign type at `ty` or inside it.
    fn reached(&self, ty: &'a Type, seen: &mut BTreeSet<&'a str>) -> Option<&'a str> {
        foreign(ty).or_else(|| self.inner_foreign(ty, seen))
    }
}

fn camel(name: &str) -> String {
    name.to_lower_camel_case()
}

/// The text after `prefix` on the one line of `binding` that starts with it.
fn declaration<'b>(binding: &'b str, prefix: &str, owner: &str) -> Result<&'b str> {
    let mut found = binding.lines().filter_map(|line| line.strip_prefix(prefix));
    let declaration = found
        .next()
        .with_context(|| format!("{owner}: the generated binding has no `{}`", prefix.trim()))?;
    if found.next().is_some() {
        bail!(
            "{owner}: the generated binding declares `{}` more than once",
            prefix.trim()
        );
    }
    Ok(declaration)
}

/// The parameters of a declaration whose text starts after its `(`, each
/// with its type, and the text after the closing `)`. Commas inside generic,
/// tuple, and collection types stay with their parameter.
fn parameter_list<'d>(text: &'d str, owner: &str) -> Result<(Vec<&'d str>, &'d str)> {
    let mut depth = 0usize;
    let mut start = 0;
    let mut previous = ' ';
    let mut parameters = Vec::new();
    for (at, c) in text.char_indices() {
        match c {
            '(' | '<' | '[' => depth += 1,
            ')' if depth == 0 => {
                let last = text[start..at].trim();
                if !last.is_empty() {
                    parameters.push(last);
                }
                return Ok((parameters, &text[at + 1..]));
            }
            // The arrow of a function type closes nothing.
            '>' if previous == '-' => {}
            ')' | '>' | ']' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parameters.push(text[start..at].trim());
                start = at + 1;
            }
            _ => {}
        }
        previous = c;
    }
    bail!("{owner}: the generated declaration has no closing parenthesis")
}

/// Each declared parameter by name, in the function's order. The names must
/// be the metadata's, so a type never lands on the wrong parameter.
fn declared<'d>(function: &FnMetadata, parameters: &[&'d str]) -> Result<Vec<(String, &'d str)>> {
    let declared = parameters
        .iter()
        .map(|parameter| {
            let name = parameter
                .split_once(':')
                .map(|(name, _)| name.trim().trim_matches('`').to_owned())
                .with_context(|| {
                    format!("{}: parameter `{parameter}` has no type", function.name)
                })?;
            Ok((name, *parameter))
        })
        .collect::<Result<Vec<_>>>()?;
    let expected = function
        .inputs
        .iter()
        .map(|input| camel(&input.name))
        .collect::<Vec<_>>();
    let found = declared
        .iter()
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    if found != expected {
        bail!(
            "{}: the generated binding declares ({}), but the metadata has ({})",
            function.name,
            found.join(", "),
            expected.join(", ")
        );
    }
    Ok(declared)
}

/// The declared parameters in the static's order.
fn reordered(item: &ClientStatic, declared: &[(String, &str)]) -> String {
    item.parameters
        .iter()
        .filter_map(|parameter| {
            let name = camel(&parameter.name);
            declared
                .iter()
                .find(|(declared, _)| *declared == name)
                .map(|(_, text)| *text)
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The argument of one Kotlin call: a foreign value goes through its
/// `SDKForeign` wrapper, which also wraps the foreign values inside it. A
/// foreign value inside anything else would reach Rust unwrapped.
fn kotlin_argument<'a>(
    owner: &str,
    parameter: &'a FnParamMetadata,
    fields: &Fields<'a>,
) -> Result<String> {
    let name = format!("`{}`", camel(&parameter.name));
    if let Some(ty) = foreign(&parameter.ty) {
        let wrapper = KOTLIN_FOREIGN
            .iter()
            .find(|(foreign, _)| *foreign == ty)
            .map(|(_, wrapper)| wrapper)
            .with_context(|| {
                format!(
                    "{owner}.{}: {ty} has no wrapper in runtime/kotlin/SDKForeign.kt",
                    parameter.name
                )
            })?;
        return Ok(format!("SDKForeign.{wrapper}({name})"));
    }
    if let Some(inner) = fields.inner_foreign(&parameter.ty, &mut BTreeSet::new()) {
        bail!(
            "{owner}.{}: it holds a {inner} that no SDKForeign wrapper reaches; take the {inner} as a parameter of its own",
            parameter.name
        );
    }
    Ok(name)
}

/// Kotlin: an extension of `SDKClient.Companion` per static.
pub(crate) fn kotlin<'a>(
    statics: &[ClientStatic<'a>],
    binding: &str,
    items: &[&'a Metadata],
) -> Result<String> {
    let fields = Fields::new(items);
    let mut code = String::new();
    for item in statics {
        let owner = &item.function.name;
        let function = camel(owner);
        let text = declaration(binding, &format!("     suspend fun `{function}`("), owner)?;
        let (parameters, rest) = parameter_list(text, owner)?;
        let declared = declared(item.function, &parameters)?;
        // ` : Result {` or ` {`.
        let result = rest
            .trim()
            .strip_suffix('{')
            .with_context(|| format!("{owner}: the generated Kotlin declaration has no body"))?
            .trim();
        let result = match result.strip_prefix(':') {
            Some(ty) => format!(": {}", ty.trim()),
            None if result.is_empty() => String::new(),
            None => bail!("{owner}: unexpected generated Kotlin result `{result}`"),
        };
        let arguments = item
            .function
            .inputs
            .iter()
            .map(|parameter| kotlin_argument(owner, parameter, &fields))
            .collect::<Result<Vec<_>>>()?
            .join(", ");
        writeln!(
            code,
            "suspend fun SDKClient.Companion.`{}`({}){result} =\n    {KOTLIN_PACKAGE}.`{function}`({arguments})\n",
            item.name,
            reordered(item, &declared)
        )?;
    }
    Ok(code)
}

/// Swift: a static member of the `SDKClient` extension per static, with
/// every parameter labelled.
/// An argument label at a call site. Swift admits any keyword there except
/// `inout`, `var` and `let`, which UniFFI quotes in the declaration too.
fn swift_label(label: &str) -> String {
    if ["inout", "var", "let"].contains(&label) {
        format!("`{label}`")
    } else {
        label.to_owned()
    }
}

pub(crate) fn swift(statics: &[ClientStatic], binding: &str) -> Result<String> {
    let mut code = String::new();
    for item in statics {
        let owner = &item.function.name;
        // UniFFI declares a keyword name in backticks; the call names it so too.
        let function = swift_identifier(&camel(owner));
        let text = declaration(binding, &format!("public func {function}("), owner)?;
        let (parameters, rest) = parameter_list(text, owner)?;
        let declared = declared(item.function, &parameters)?;
        // `async throws  -> Result  {` or `async throws   {`.
        let effects = rest
            .trim()
            .strip_suffix('{')
            .with_context(|| format!("{owner}: the generated Swift declaration has no body"))?
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let call = match (effects.starts_with("async"), effects.contains("throws")) {
            (true, true) => "try await ",
            (true, false) => "await ",
            _ => bail!("{owner}: the generated Swift function is not async"),
        };
        let arguments = declared
            .iter()
            .map(|(label, text)| {
                let value = text
                    .split_once(':')
                    .map_or(*text, |(value, _)| value)
                    .trim()
                    .trim_matches('`');
                format!("{}: {}", swift_label(label), swift_identifier(value))
            })
            .collect::<Vec<_>>()
            .join(", ");
        writeln!(
            code,
            "    static func `{}`({}) {effects} {{\n        {call}{SWIFT_MODULE}.{function}({arguments})\n    }}\n",
            item.name,
            reordered(item, &declared)
        )?;
    }
    Ok(code)
}

#[cfg(test)]
mod tests;
