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
/// named, and the properties of every JavaScript function, which a
/// TypeScript class is.
const TAKEN_STATICS: &[(&str, &str)] = &[
    ("arguments", "a property of every JavaScript function"),
    ("build", "a host constructor"),
    ("caller", "a property of every JavaScript function"),
    ("constructor", "the class constructor"),
    ("create", "a host constructor"),
    ("length", "a property of every JavaScript function"),
    ("name", "a property of every JavaScript function"),
    ("prototype", "a property of every JavaScript function"),
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
mod tests {
    use uniffi_meta::{DefaultValueMetadata, LiteralMetadata, ObjectImpl, TraitKind, Type};

    use super::*;
    use crate::test_metadata::{
        enum_type, enumeration, field, optional, record, record_type, variant,
    };

    fn input(name: &str, ty: Type) -> FnParamMetadata {
        FnParamMetadata::simple(name, ty)
    }

    fn backend() -> Type {
        Type::Enum {
            module_path: "xmtp_sdk".into(),
            name: BACKEND_SOURCE.into(),
        }
    }

    fn signer() -> Type {
        Type::Object {
            module_path: "xmtp_sdk".into(),
            name: "Signer".into(),
            imp: ObjectImpl::Trait(TraitKind::Both),
        }
    }

    fn function(name: &str, inputs: Vec<FnParamMetadata>, doc: Option<&str>) -> Metadata {
        Metadata::Func(FnMetadata {
            module_path: "xmtp_sdk".into(),
            name: name.into(),
            orig_name: None,
            is_async: true,
            inputs,
            return_type: Some(Type::Boolean),
            throws: None,
            checksum: None,
            docstring: doc.map(Into::into),
        })
    }

    fn marked(name: &str, inputs: Vec<FnParamMetadata>) -> Metadata {
        function(
            name,
            inputs,
            Some("Reads the backend.\n@xmtp-client-static"),
        )
    }

    fn statics(items: &[Metadata]) -> Result<Vec<(String, Vec<String>)>> {
        let refs = items.iter().collect::<Vec<_>>();
        Ok(client_statics(&refs)?
            .into_iter()
            .map(|item| {
                (
                    item.name,
                    item.parameters
                        .iter()
                        .map(|parameter| parameter.name.clone())
                        .collect(),
                )
            })
            .collect())
    }

    // The rule: the name loses `_with_backend`, and the backend moves last.
    #[xmtp_common::test(unwrap_try = true)]
    fn statics_drop_the_backend_suffix_and_take_the_backend_last() {
        let items = [
            marked(
                "is_address_authorized_with_backend",
                vec![
                    input("backend", backend()),
                    input("inbox_id", Type::String),
                    input("address", Type::String),
                ],
            ),
            marked(
                "fetch_server_configuration",
                vec![input("backend", backend())],
            ),
            marked(
                "verify_signed_with_public_key",
                vec![input("text", Type::String), input("signature", Type::Bytes)],
            ),
            // An unmarked backend function stays a plain function.
            function(
                "latest_inbox_updates_count",
                vec![
                    input("inbox_ids", Type::String),
                    input("backend", backend()),
                ],
                None,
            ),
        ];
        assert_eq!(
            statics(&items)?,
            [
                (
                    "fetchServerConfiguration".to_owned(),
                    vec!["backend".to_owned()]
                ),
                (
                    "isAddressAuthorized".to_owned(),
                    vec!["inbox_id".into(), "address".into(), "backend".into()]
                ),
                (
                    "verifySignedWithPublicKey".to_owned(),
                    vec!["text".into(), "signature".into()]
                ),
            ]
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn statics_stop_on_a_function_the_rule_cannot_express() {
        let error = |items: Vec<Metadata>| statics(&items).unwrap_err().to_string();
        let mut sync = marked("inbox_states_with_backend", vec![]);
        if let Metadata::Func(function) = &mut sync {
            function.is_async = false;
        }
        assert!(error(vec![sync]).contains("asynchronous"));
        assert!(
            error(vec![function(
                "inbox_states_with_backend",
                vec![],
                Some("@xmtp-client-static @xmtp-internal")
            )])
            .contains("cannot be @xmtp-internal")
        );
        assert!(
            error(vec![marked(
                "copy_with_backend",
                vec![input("from", backend()), input("to", backend())]
            )])
            .contains("at most one BackendSource")
        );
        assert!(error(vec![marked("create_with_backend", vec![])]).contains("host constructor"));
        // TypeScript reads `static constructor` as a constructor declaration.
        assert!(
            error(vec![marked("constructor_with_backend", vec![])])
                .contains("Client.constructor is the class constructor")
        );
        // A TypeScript class is a function, and its own properties are taken.
        for name in ["name", "length", "prototype", "caller", "arguments"] {
            let message = error(vec![marked(&format!("{name}_with_backend"), vec![])]);
            assert!(
                message.contains("a property of every JavaScript function"),
                "{name}: {message}"
            );
        }
        assert!(
            error(vec![
                marked("inbox_states", vec![]),
                marked("inbox_states_with_backend", vec![]),
            ])
            .contains("also Client.inboxStates")
        );
        for name in ["_with_backend", "Inbox_with_backend", "inbóx"] {
            assert!(error(vec![marked(name, vec![])]).contains("lowercase ASCII"));
        }
    }

    fn defaulted(name: &str, ty: Type) -> FnParamMetadata {
        let mut parameter = input(name, ty);
        parameter.default = Some(DefaultValueMetadata::Literal(LiteralMetadata::None));
        parameter
    }

    // A caller may leave out a trailing defaulted parameter of the function,
    // and of its static too. The moved backend would come after it, and
    // TypeScript cannot leave out a parameter before a required one.
    #[xmtp_common::test(unwrap_try = true)]
    fn statics_keep_a_defaulted_parameter_optional_or_stop() {
        let nonce = || {
            defaulted(
                "nonce",
                Type::Optional {
                    inner_type: Box::new(Type::UInt64),
                },
            )
        };
        let error = statics(&[marked(
            "inbox_states_with_backend",
            vec![
                input("backend", backend()),
                input("ids", Type::String),
                nonce(),
            ],
        )])
        .unwrap_err()
        .to_string();
        assert!(
            error.starts_with("inbox_states_with_backend.nonce: the static takes the BackendSource after this defaulted parameter"),
            "{error}"
        );
        let mut defaulted_backend = input("backend", backend());
        defaulted_backend.default = Some(DefaultValueMetadata::Default);
        let error = statics(&[marked(
            "inbox_states_with_backend",
            vec![defaulted_backend, input("ids", Type::String), nonce()],
        )])
        .unwrap_err()
        .to_string();
        assert!(
            error.starts_with(
                "inbox_states_with_backend.backend: a client static's BackendSource cannot have a default"
            ),
            "{error}"
        );
        // Without a backend the order stays, and so does the default.
        let items = [marked(
            "inbox_states",
            vec![input("ids", Type::String), nonce()],
        )];
        let refs = items.iter().collect::<Vec<_>>();
        let code = crate::public_projection::client_members_for_test(&refs)?;
        assert!(
            code.contains("static inboxStates(ids: string, nonce?: bigint): Promise<boolean> {\nreturn inboxStates(ids, nonce);\n}"),
            "{code}"
        );
    }

    // The public Client inherits the statics of the generated ClientMembers.
    #[xmtp_common::test(unwrap_try = true)]
    fn typescript_statics_call_the_public_function_with_the_backend_last() {
        let items = [
            marked(
                "is_address_authorized_with_backend",
                vec![
                    input("backend", backend()),
                    input("inbox_id", Type::String),
                    input("address", Type::String),
                ],
            ),
            function(
                "latest_inbox_updates_count",
                vec![
                    input("inbox_ids", Type::String),
                    input("backend", backend()),
                ],
                None,
            ),
        ];
        let refs = items.iter().collect::<Vec<_>>();
        let code = crate::public_projection::client_members_for_test(&refs)?;
        assert!(code.contains(
            "static isAddressAuthorized(inboxId: string, address: string, backend: BackendSource): Promise<boolean> {\nreturn isAddressAuthorizedWithBackend(backend, inboxId, address);\n}"
        ));
        assert_eq!(code.matches("static ").count(), 1);
        assert!(code.ends_with("}\n}\n"));
    }

    const KOTLIN: &str = "\
    @Throws(XmtpException::class)
     suspend fun `isAddressAuthorizedWithBackend`(`backend`: BackendSource, `inboxId`: InboxId, `address`: kotlin.String) : kotlin.Boolean {
    }
     suspend fun `revokeInstallationsWithBackend`(`backend`: BackendSource, `signer`: Signer, `ids`: Map<kotlin.String, List<InstallationId>>) {
    }
     suspend fun `create`(`signer`: Signer) : Client {
";

    #[xmtp_common::test(unwrap_try = true)]
    fn kotlin_statics_extend_the_companion_and_wrap_foreign_values() {
        let items = [
            marked(
                "is_address_authorized_with_backend",
                vec![
                    input("backend", backend()),
                    input("inbox_id", Type::String),
                    input("address", Type::String),
                ],
            ),
            marked(
                "revoke_installations_with_backend",
                vec![
                    input("backend", backend()),
                    input("signer", signer()),
                    input("ids", Type::String),
                ],
            ),
        ];
        let refs = items.iter().collect::<Vec<_>>();
        let code = kotlin(&client_statics(&refs)?, KOTLIN, &refs)?;
        assert_eq!(
            code,
            "\
suspend fun SDKClient.Companion.`isAddressAuthorized`(`inboxId`: InboxId, `address`: kotlin.String, `backend`: BackendSource): kotlin.Boolean =
    uniffi.xmtp_sdk.`isAddressAuthorizedWithBackend`(SDKForeign.backend(`backend`), `inboxId`, `address`)

suspend fun SDKClient.Companion.`revokeInstallations`(`signer`: Signer, `ids`: Map<kotlin.String, List<InstallationId>>, `backend`: BackendSource) =
    uniffi.xmtp_sdk.`revokeInstallationsWithBackend`(SDKForeign.backend(`backend`), SDKForeign.signer(`signer`), `ids`)

"
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn kotlin_statics_stop_on_a_binding_they_cannot_read() {
        let render = |items: Vec<Metadata>, binding: &str| {
            let refs = items.iter().collect::<Vec<_>>();
            kotlin(&client_statics(&refs).unwrap(), binding, &refs)
                .unwrap_err()
                .to_string()
        };
        let address = || {
            marked(
                "is_address_authorized_with_backend",
                vec![
                    input("backend", backend()),
                    input("inbox_id", Type::String),
                    input("address", Type::String),
                ],
            )
        };
        assert!(
            render(vec![address()], "")
                .contains("has no `suspend fun `isAddressAuthorizedWithBackend`(`")
        );
        assert!(render(vec![address()], &format!("{KOTLIN}{KOTLIN}")).contains("more than once"));
        // A type never lands on another parameter.
        assert!(
            render(
                vec![marked(
                    "is_address_authorized_with_backend",
                    vec![
                        input("backend", backend()),
                        input("address", Type::String),
                        input("inbox_id", Type::String),
                    ],
                )],
                KOTLIN
            )
            .contains("declares (backend, inboxId, address), but the metadata has (backend, address, inboxId)")
        );
        // A foreign trait without a wrapper, or inside a container, stops.
        let listener = Type::CallbackInterface {
            module_path: "xmtp_sdk".into(),
            name: "EventListener".into(),
        };
        let binding = "     suspend fun `watchWithBackend`(`backend`: BackendSource, `listener`: EventListener) {\n";
        assert!(
            render(
                vec![marked(
                    "watch_with_backend",
                    vec![input("backend", backend()), input("listener", listener)]
                )],
                binding
            )
            .contains("EventListener has no wrapper")
        );
        let binding = "     suspend fun `watchWithBackend`(`backend`: BackendSource, `signers`: List<Signer>) {\n";
        assert!(
            render(
                vec![marked(
                    "watch_with_backend",
                    vec![
                        input("backend", backend()),
                        input(
                            "signers",
                            Type::Sequence {
                                inner_type: Box::new(signer())
                            }
                        )
                    ]
                )],
                binding
            )
            .contains("signers: it holds a Signer that no SDKForeign wrapper reaches")
        );
    }

    // A record or enum that holds a foreign value has no wrapper: the value
    // would reach Rust unwrapped. BackendOptions holds a CredentialSource,
    // and ClientOptions a BackendSource that holds BackendOptions.
    #[xmtp_common::test(unwrap_try = true)]
    fn kotlin_statics_stop_on_a_foreign_value_inside_a_record_or_enum() {
        let credentials = Type::CallbackInterface {
            module_path: "xmtp_sdk".into(),
            name: "CredentialSource".into(),
        };
        let types = || {
            vec![
                record(
                    "BackendOptions",
                    vec![
                        field("url", Type::String, None),
                        field("credentials", optional(credentials.clone()), None),
                    ],
                ),
                enumeration(
                    BACKEND_SOURCE,
                    vec![variant(
                        "Options",
                        None,
                        vec![field("options", record_type("BackendOptions"), None)],
                    )],
                ),
                record(
                    "ClientOptions",
                    vec![field("backend", optional(backend()), None)],
                ),
                record(
                    "PublicIdentity",
                    vec![field("identifier", Type::String, None)],
                ),
                enumeration(
                    "SignIn",
                    vec![variant(
                        "Remote",
                        None,
                        vec![field("credentials", credentials.clone(), None)],
                    )],
                ),
            ]
        };
        let render = |parameter: &str, ty: Type| {
            let mut items = types();
            items.push(marked("probe_with_backend", vec![input(parameter, ty)]));
            let refs = items.iter().collect::<Vec<_>>();
            let binding = format!("     suspend fun `probeWithBackend`(`{parameter}`: T) {{\n");
            kotlin(&client_statics(&refs)?, &binding, &refs)
        };
        let error = render("options", record_type("BackendOptions"))
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("probe_with_backend.options: it holds a CredentialSource that no SDKForeign wrapper reaches"),
            "{error}"
        );
        let error = render("options", record_type("ClientOptions"))
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("options: it holds a BackendSource that no SDKForeign wrapper reaches"),
            "{error}"
        );
        let error = render("choice", enum_type("SignIn"))
            .unwrap_err()
            .to_string();
        assert!(
            error
                .contains("choice: it holds a CredentialSource that no SDKForeign wrapper reaches"),
            "{error}"
        );
        // The backend's own wrapper reaches the credentials inside it, and a
        // record without a foreign value passes as it is.
        assert!(render("backend", backend())?.contains("(SDKForeign.backend(`backend`))"));
        assert!(
            render("identity", record_type("PublicIdentity"))?
                .contains("`probeWithBackend`(`identity`)")
        );
    }

    // A Swift keyword stays quoted wherever the call names it: the module
    // function, and an argument used as an expression.
    #[xmtp_common::test(unwrap_try = true)]
    fn swift_statics_quote_keyword_names() {
        let binding = "\
public func `switch`(backend: BackendSource, default: Int32, `var`: Bool)async throws   {
";
        let items = [marked(
            "switch",
            vec![
                input("backend", backend()),
                input("default", Type::Int32),
                input("var", Type::Boolean),
            ],
        )];
        let refs = items.iter().collect::<Vec<_>>();
        let code = swift(&client_statics(&refs)?, binding)?;
        assert!(
            code.contains(
                "try await XmtpSdk.`switch`(backend: backend, default: `default`, `var`: `var`)"
            ),
            "{code}"
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn swift_statics_label_every_parameter_and_call_the_module_function() {
        let binding = "\
public func fetchServerConfiguration(backend: BackendSource)async throws  -> ServerConfiguration  {
public func canMessageWithBackend(backend: BackendSource, identities: [PublicIdentity], options: [String: Bool]? = nil)async throws  -> [String: Bool]  {
public func revokeInstallationsWithBackend(backend: BackendSource, signer: Signer)async throws   {
";
        let items = [
            marked(
                "fetch_server_configuration",
                vec![input("backend", backend())],
            ),
            marked(
                "can_message_with_backend",
                vec![
                    input("backend", backend()),
                    input("identities", Type::String),
                    input("options", Type::String),
                ],
            ),
            marked(
                "revoke_installations_with_backend",
                vec![input("backend", backend()), input("signer", signer())],
            ),
        ];
        let refs = items.iter().collect::<Vec<_>>();
        assert_eq!(
            swift(&client_statics(&refs)?, binding)?,
            "    static func `canMessage`(identities: [PublicIdentity], options: [String: Bool]? = nil, backend: BackendSource) async throws -> [String: Bool] {
        try await XmtpSdk.canMessageWithBackend(backend: backend, identities: identities, options: options)
    }

    static func `fetchServerConfiguration`(backend: BackendSource) async throws -> ServerConfiguration {
        try await XmtpSdk.fetchServerConfiguration(backend: backend)
    }

    static func `revokeInstallations`(signer: Signer, backend: BackendSource) async throws {
        try await XmtpSdk.revokeInstallationsWithBackend(backend: backend, signer: signer)
    }

"
        );
        let refs = items[..1].iter().collect::<Vec<_>>();
        assert!(
            swift(&client_statics(&refs)?, "public func fetchServerConfiguration(backend: BackendSource) -> ServerConfiguration {\n")
                .unwrap_err()
                .to_string()
                .contains("not async")
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn parameter_lists_keep_nested_commas_and_arrows() {
        let (parameters, rest) = parameter_list(
            "a: Map<K, V>, b: (Int, Int) -> Void, c: [String: [Int]], d: Map<(Int) -> Void, Int>) async {",
            "f",
        )?;
        assert_eq!(
            parameters,
            [
                "a: Map<K, V>",
                "b: (Int, Int) -> Void",
                "c: [String: [Int]]",
                "d: Map<(Int) -> Void, Int>"
            ]
        );
        assert_eq!(rest, " async {");
        assert_eq!(parameter_list(") {", "f")?, (vec![], " {"));
        assert!(parameter_list("a: Int", "f").is_err());
    }
}
