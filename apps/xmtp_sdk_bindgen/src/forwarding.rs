use std::{collections::BTreeMap, fs};

use anyhow::{Context, Result, bail};
use camino::Utf8Path;
use heck::ToLowerCamelCase;
use uniffi_meta::{Metadata, MetadataGroupMap, MethodMetadata};

use crate::{Language, client_statics};

/// Select only methods with the same complete Rust signature on both objects.
fn common_methods(groups: &MetadataGroupMap) -> Vec<String> {
    common_methods_from_items(groups.values().flat_map(|group| &group.items))
}

fn common_methods_from_items<'a>(items: impl IntoIterator<Item = &'a Metadata>) -> Vec<String> {
    let mut group = BTreeMap::new();
    let mut dm = BTreeMap::new();
    for metadata in items {
        let Metadata::Method(method) = metadata else {
            continue;
        };
        if crate::markers::has(method.docstring.as_deref(), crate::markers::INTERNAL) {
            continue;
        }
        match method.self_name.as_str() {
            "Group" => {
                group.insert(method.name.as_str(), method);
            }
            "Dm" => {
                dm.insert(method.name.as_str(), method);
            }
            _ => {}
        }
    }
    group
        .into_iter()
        .filter_map(|(name, method)| {
            dm.get(name)
                .filter(|other| same_signature(method, other))
                .map(|_| host_name(name))
        })
        .collect()
}

fn same_signature(a: &MethodMetadata, b: &MethodMetadata) -> bool {
    a.is_async == b.is_async
        && format!("{:?}", a.return_type) == format!("{:?}", b.return_type)
        && format!("{:?}", a.throws) == format!("{:?}", b.throws)
        && a.inputs.len() == b.inputs.len()
        && a.inputs.iter().zip(&b.inputs).all(|(left, right)| {
            left.name == right.name && format!("{:?}", left.ty) == format!("{:?}", right.ty)
        })
}

fn host_name(name: &str) -> String {
    name.to_lower_camel_case()
}

fn declarations(source: &str, start: &str, prefix: &str) -> Result<BTreeMap<String, String>> {
    let body = source
        .split_once(start)
        .with_context(|| format!("generated binding has no {start}"))?
        .1
        .split_once("\n}")
        .with_context(|| format!("generated binding has no end for {start}"))?
        .0;
    Ok(body
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let declaration = line.strip_prefix(prefix).or_else(|| {
                line.strip_prefix("suspend ")
                    .and_then(|rest| rest.strip_prefix(prefix))
            })?;
            let name = declaration.split_once('(')?.0.trim_matches('`');
            Some((name.to_string(), line.to_string()))
        })
        .collect())
}

fn arguments(declaration: &str, swift: bool) -> Vec<String> {
    let Some(parameters) = declaration
        .split_once('(')
        .and_then(|(_, rest)| rest.split_once(')'))
    else {
        return Vec::new();
    };
    parameters
        .0
        .split(',')
        .filter_map(|parameter| {
            parameter
                .split_once(':')
                .map(|(name, _)| name.trim().trim_matches('`'))
        })
        .map(|name| {
            if swift {
                format!("{name}: {name}")
            } else {
                name.to_string()
            }
        })
        .collect()
}

fn render(
    selected: &[String],
    group: &BTreeMap<String, String>,
    dm: &BTreeMap<String, String>,
    language: Language,
) -> Result<String> {
    let swift = matches!(language, Language::Swift);
    let mut methods = String::new();
    for name in selected {
        let (Some(g), Some(d)) = (group.get(name), dm.get(name)) else {
            bail!("{name}: common method is missing from generated bindings");
        };
        if g != d {
            // The stock generator may project the same Rust type differently.
            continue;
        }
        let args = arguments(g, swift).join(", ");
        if swift {
            let signature = g.strip_prefix("func ").expect("Swift declaration");
            let effect = if signature.contains("async throws") {
                "try await "
            } else if signature.contains(" async ") {
                "await "
            } else if signature.contains(" throws ") {
                "try "
            } else {
                ""
            };
            methods.push_str(&format!(
                "    func {signature} {{\n        switch self {{\n        case .group(let group): return {effect}group.{name}({args})\n        case .dm(let dm): return {effect}dm.{name}({args})\n        }}\n    }}\n\n"
            ));
        } else {
            let signature = g.replace("fun `", "fun Conversation.`").replace('`', "");
            methods.push_str(&format!(
                "{signature} = when (this) {{\n    is Conversation.Group -> group.{name}({args})\n    is Conversation.Dm -> dm.{name}({args})\n}}\n\n"
            ));
        }
    }
    Ok(methods)
}

/// Client methods that the host Client implements itself, or that stay private
/// because they identify a transport handle.
const HOST_CLIENT_METHODS: &[&str] = &[
    "clientKey",
    "end",
    "events",
    "startListener",
    "stopListener",
    "storage",
];

/// Select the host names of every exported Client instance method that the host
/// Client must forward.
fn client_methods<'a>(items: impl IntoIterator<Item = &'a Metadata>) -> Vec<String> {
    let mut names = items
        .into_iter()
        .filter_map(|item| match item {
            Metadata::Method(method) if method.self_name == "Client" => {
                Some(host_name(&method.name))
            }
            _ => None,
        })
        .filter(|name| !HOST_CLIENT_METHODS.contains(&name.as_str()))
        .collect::<Vec<_>>();
    names.sort();
    names
}

/// Swift class methods carry their default arguments; the protocol does not.
fn swift_client_declarations(source: &str) -> Result<BTreeMap<String, String>> {
    let body = source
        .split_once("open class Client:")
        .context("generated Swift binding has no Client class")?
        .1
        .split_once("public struct FfiConverterTypeClient")
        .context("generated Swift binding has no end for the Client class")?
        .0;
    Ok(body
        .lines()
        .filter_map(|line| {
            let declaration = line.strip_prefix("open func ")?;
            let name = declaration.split_once('(')?.0.trim_matches('`');
            Some((name.to_string(), line.to_string()))
        })
        .collect())
}

fn render_client(
    selected: &[String],
    declarations: &BTreeMap<String, String>,
    language: Language,
) -> Result<String> {
    let swift = matches!(language, Language::Swift);
    let mut methods = String::new();
    for name in selected {
        let Some(declaration) = declarations.get(name) else {
            bail!("Client.{name}: exported method is missing from generated bindings");
        };
        let args = arguments(declaration, swift).join(", ");
        if swift {
            let signature = declaration
                .trim_start_matches("open func ")
                .trim_end()
                .trim_end_matches('{')
                .replace(")async", ") async")
                .replace(")throws", ") throws")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            let effect = if signature.contains(" async throws") {
                "try await "
            } else if signature.contains(" async") {
                "await "
            } else if signature.contains(" throws") {
                "try "
            } else {
                ""
            };
            methods.push_str(&format!(
                "    func {signature} {{\n        {effect}raw.{name}({args})\n    }}\n\n"
            ));
        } else {
            let signature = declaration.replace("fun `", "fun SDKClient.`");
            methods.push_str(&format!("{signature} = raw.`{name}`({args})\n\n"));
        }
    }
    Ok(methods)
}

fn generate_client(
    groups: &MetadataGroupMap,
    language: Language,
    source: &str,
    out: &Utf8Path,
) -> Result<()> {
    let items = groups
        .values()
        .flat_map(|group| &group.items)
        .collect::<Vec<_>>();
    let selected = client_methods(items.iter().copied())
        .into_iter()
        .filter(|name| name != "conversations")
        .collect::<Vec<_>>();
    let statics = client_statics::client_statics(&items)?;
    let (found, statics, header, footer, filename) = match language {
        Language::Swift => (
            swift_client_declarations(source)?,
            client_statics::swift(&statics, source)?,
            "/// Generated from exported Client methods and client statics. Do not edit\n/// this output.\nimport Foundation\n\npublic extension SDKClient {\n".to_owned(),
            "}\n",
            "ClientForwarding.swift",
        ),
        Language::Kotlin => (
            declarations(source, "public interface ClientInterface {", "fun ")?,
            client_statics::kotlin(&statics, source, &items)?,
            format!(
                "// Generated from exported Client methods and client statics. Do not edit\n// this output.\npackage {}\n\n",
                client_statics::KOTLIN_PACKAGE
            ),
            "",
            "ClientForwarding.kt",
        ),
        _ => bail!("Client forwarding needs Swift or Kotlin"),
    };
    let rendered = render_client(&selected, &found, language)? + &statics;
    fs::write(
        out.join("runtime").join(filename),
        format!(
            "{header}{}{footer}",
            rendered.trim_end_matches('\n').to_owned() + "\n"
        ),
    )?;
    Ok(())
}

/// Apps construct the host Client. Keep the generated Client factories visible
/// only to the runtime module, so they cannot bypass its registry and codecs.
fn hide_client_factories(source: &str, language: Language) -> Result<String> {
    let replacements: &[(&str, &str)] = match language {
        Language::Swift => &[
            (
                "\npublic static func build(identity: PublicIdentity,",
                "\nstatic func build(identity: PublicIdentity,",
            ),
            (
                "\npublic static func create(signer: Signer,",
                "\nstatic func create(signer: Signer,",
            ),
        ],
        Language::Kotlin => &[
            (
                "     suspend fun `build`(`identity`: PublicIdentity,",
                "     internal suspend fun `build`(`identity`: PublicIdentity,",
            ),
            (
                "     suspend fun `create`(`signer`: Signer,",
                "     internal suspend fun `create`(`signer`: Signer,",
            ),
        ],
        _ => bail!("Client factories are hidden only in Swift and Kotlin"),
    };
    let mut output = source.to_owned();
    for (from, to) in replacements {
        if output.matches(from).count() != 1 {
            bail!("generated Client factory changed shape: {}", from.trim());
        }
        output = output.replacen(from, to, 1);
    }
    Ok(output)
}

/// Account-identity membership methods. The public API is the same-name
/// overload in the host runtime, so the generated method is internal to it.
const IDENTITY_ROUTES: &[&str] = &[
    "createGroupWithIdentities",
    "createDmWithIdentity",
    "addMembersByIdentity",
    "removeMembersByIdentity",
];

/// Remove each identity route from its generated protocol or interface, and
/// make the generated class method internal.
fn hide_identity_routes(source: &str, language: Language) -> Result<String> {
    // Each template names the method with `NAME`.
    let (declaration, method, internal) = match language {
        Language::Swift => ("    func NAME(", "open func NAME(", "func NAME("),
        Language::Kotlin => (
            "    suspend fun `NAME`(",
            "    override suspend fun `NAME`(",
            "    internal suspend fun `NAME`(",
        ),
        _ => bail!("identity routes are hidden only in Swift and Kotlin"),
    };
    let internal = |name: &str| internal.replace("NAME", name);
    let (declaration, method) = (
        |name: &str| declaration.replace("NAME", name),
        |name: &str| method.replace("NAME", name),
    );
    let mut output = source.to_owned();
    for name in IDENTITY_ROUTES {
        let (declaration, method) = (declaration(name), method(name));
        let declared = output
            .lines()
            .filter(|line| line.starts_with(&declaration))
            .count();
        if declared != 1 || output.matches(&method).count() != 1 {
            bail!("generated identity route changed shape: {name}");
        }
        output = output
            .lines()
            .filter(|line| !line.starts_with(&declaration))
            .map(|line| format!("{line}\n"))
            .collect::<String>()
            .replacen(&method, &internal(name), 1);
    }
    Ok(output)
}

/// The proxy carries only methods that can cross the worker bridge.
pub(crate) fn bridged_interface(items: &[&Metadata], owner: &str, stock: &str) -> String {
    let mut omitted = items
        .iter()
        .filter_map(|item| match item {
            Metadata::Method(method)
                if method.self_name == owner && crate::bridge::worker_only(item) =>
            {
                Some(host_name(&method.name))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    omitted.sort();
    omitted.dedup();
    if omitted.is_empty() {
        return stock.to_owned();
    }
    format!(
        "Omit<{stock}, {}>",
        omitted
            .iter()
            .map(|name| format!("\"{name}\""))
            .collect::<Vec<_>>()
            .join(" | ")
    )
}

pub(crate) fn typescript_forwarders(items: &[&Metadata]) -> String {
    let selected =
        client_methods(items.iter().copied().filter(
            |item| !matches!(item, Metadata::Method(_) if crate::bridge::worker_only(item)),
        ));
    render_typescript(&selected, &bridged_interface(items, "Client", "ClientLike"))
}

/// Write the TypeScript host Client's forwarders as a base class. Each method
/// takes and returns the binding method's own types, so a method that the
/// binding lacks fails the TypeScript compile.
pub(crate) fn generate_typescript(groups: &MetadataGroupMap, out: &Utf8Path) -> Result<()> {
    let items = groups
        .values()
        .flat_map(|group| &group.items)
        .collect::<Vec<_>>();
    let name = "client-forwarding.gen.ts";
    fs::write(
        out.join(name),
        crate::format::typescript(name, &typescript_forwarders(&items))?,
    )?;
    Ok(())
}

fn render_typescript(selected: &[String], binding: &str) -> String {
    let mut code = format!("export type ClientBinding = {binding};\n");
    code.push_str(
        "// Generated from exported Client methods. Do not edit this output.\nimport type { ClientLike } from \"./xmtp_sdk\";\n\n/** The host Client forwards these methods to its private binding Client. */\nexport abstract class ClientForwarders {\n  protected abstract binding(): ClientBinding;\n",
    );
    for name in selected {
        code.push_str(&format!(
            "\n  {name}(\n    ...args: Parameters<ClientLike[\"{name}\"]>\n  ): ReturnType<ClientLike[\"{name}\"]> {{\n    return this.binding().{name}(...args);\n  }}\n"
        ));
    }
    code.push_str("}\n");
    code
}

pub(crate) fn generate(
    groups: &MetadataGroupMap,
    language: Language,
    out: &Utf8Path,
) -> Result<()> {
    let selected = common_methods(groups);
    let (binding, group_start, dm_start, prefix, template, filename) = match language {
        Language::Swift => (
            out.join("xmtp_sdk.swift"),
            "public protocol GroupProtocol:",
            "public protocol DmProtocol:",
            "func ",
            include_str!("../templates/swift/conversation_forwarding.swift"),
            "ConversationForwarding.swift",
        ),
        Language::Kotlin => (
            out.join("uniffi/xmtp_sdk/xmtp_sdk.kt"),
            "public interface GroupInterface {",
            "public interface DmInterface {",
            "fun ",
            include_str!("../templates/kotlin/conversation_forwarding.kt"),
            "ConversationForwarding.kt",
        ),
        _ => bail!("conversation forwarding needs Swift or Kotlin"),
    };
    let source = hide_identity_routes(
        &hide_client_factories(
            &fs::read_to_string(&binding).with_context(|| format!("read {binding}"))?,
            language,
        )?,
        language,
    )?;
    fs::write(&binding, &source)?;
    generate_client(groups, language, &source, out)?;
    let group = declarations(&source, group_start, prefix)?;
    let dm = declarations(&source, dm_start, prefix)?;
    let rendered = render(&selected, &group, &dm, language)?;
    if rendered.is_empty() {
        bail!("no common Conversation methods were generated");
    }
    fs::write(
        out.join("runtime").join(filename),
        template.replace(
            if matches!(language, Language::Swift) {
                "    // __CONVERSATION_METHODS__"
            } else {
                "private object ConversationForwardingTemplate"
            },
            &rendered,
        ),
    )?;
    Ok(())
}

/// Host-only streams use the same common-receiver boundary as Rust methods.
/// Their declarations stay in StreamMethods to preserve native API locations.
pub(crate) fn stream_methods(
    streams: &[crate::streams::Stream],
    language: Language,
) -> Result<String> {
    let mut code = String::new();
    for stream in crate::streams::common(streams, "Group", "Dm")? {
        let (name, options) = (stream.host_name(), &stream.options);
        let doc = crate::streams::documentation(stream, language);
        match language {
            Language::Swift => code.push_str(&format!("\npublic extension Conversation {{\n{doc}    func {name}(options: {options} = .init()) async throws -> {result} {{\n        switch self {{\n        case let .group(group): return try await group.{name}(options: options)\n        case let .dm(dm): return try await dm.{name}(options: options)\n        }}\n    }}\n}}\n", result = stream.swift_result())),
            Language::Kotlin => code.push_str(&format!("\n{doc}fun Conversation.{name}(options: {options} = {options}()): Flow<{result}> =\n    when (this) {{\n        is Conversation.Group -> group.{name}(options)\n        is Conversation.Dm -> dm.{name}(options)\n    }}\n", result = stream.kotlin_result())),
            _ => bail!("common stream forwarding needs a native target"),
        }
    }
    Ok(code)
}

#[cfg(test)]
mod tests {
    use super::*;
    use uniffi_meta::{FnParamMetadata, Type};

    fn method(owner: &str, name: &str, ret: Option<Type>) -> MethodMetadata {
        MethodMetadata {
            module_path: "test".into(),
            self_name: owner.into(),
            name: name.into(),
            orig_name: None,
            is_async: true,
            inputs: Vec::<FnParamMetadata>::new(),
            return_type: ret,
            throws: None,
            takes_self_by_arc: true,
            checksum: None,
            docstring: None,
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn common_method_forwards_and_one_sided_method_does_not() {
        let items = vec![
            Metadata::Method(method("Group", "send_text", Some(Type::String))),
            Metadata::Method(method("Dm", "send_text", Some(Type::String))),
            Metadata::Method(method("Group", "add_members", None)),
        ];
        let selected = common_methods_from_items(&items);
        assert_eq!(selected, vec!["sendText".to_string()]);
        let group = BTreeMap::from([
            (
                "sendText".into(),
                "func sendText(text: String) async throws -> MessageId".into(),
            ),
            ("addMembers".into(), "func addMembers() async throws".into()),
        ]);
        let dm = BTreeMap::from([(
            "sendText".into(),
            "func sendText(text: String) async throws -> MessageId".into(),
        )]);
        let output = render(&selected, &group, &dm, Language::Swift).unwrap();
        assert!(output.contains("group.sendText(text: text)"));
        assert!(output.contains("dm.sendText(text: text)"));
        assert!(!output.contains("addMembers"));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn every_exported_client_method_is_forwarded_except_host_methods() {
        let items = vec![
            Metadata::Method(method("Client", "catch_up_to_live", None)),
            Metadata::Method(method("Client", "client_key", None)),
            Metadata::Method(method("Client", "end", None)),
            Metadata::Method(method("Group", "sync", None)),
        ];
        let selected = client_methods(&items);
        assert_eq!(selected, vec!["catchUpToLive".to_string()]);
        let swift = swift_client_declarations(
            "open class Client: ClientProtocol {\nopen func catchUpToLive(timeoutMs: UInt64? = nil)async throws  -> CatchUpSummary  {\n}\npublic struct FfiConverterTypeClient {}",
        )?;
        let output = render_client(&selected, &swift, Language::Swift)?;
        assert!(output.contains(
            "func catchUpToLive(timeoutMs: UInt64? = nil) async throws -> CatchUpSummary {\n        try await raw.catchUpToLive(timeoutMs: timeoutMs)"
        ));
        let kotlin = BTreeMap::from([(
            "catchUpToLive".into(),
            "suspend fun `catchUpToLive`(`timeoutMs`: kotlin.ULong? = null): CatchUpSummary".into(),
        )]);
        let output = render_client(&selected, &kotlin, Language::Kotlin)?;
        assert!(output.contains(
            "suspend fun SDKClient.`catchUpToLive`(`timeoutMs`: kotlin.ULong? = null): CatchUpSummary = raw.`catchUpToLive`(timeoutMs)"
        ));
        let missing = render_client(&selected, &BTreeMap::new(), Language::Kotlin).unwrap_err();
        assert!(missing.to_string().contains("Client.catchUpToLive"));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn client_factories_are_private_to_the_runtime() {
        let swift = "open class Client {\npublic static func build(identity: PublicIdentity, options: ClientOptions)\npublic static func create(signer: Signer, options: ClientOptions)\n}";
        let hidden = hide_client_factories(swift, Language::Swift)?;
        assert!(!hidden.contains("public static func"));
        assert!(hidden.contains("\nstatic func create(signer: Signer,"));
        let kotlin = "     suspend fun `build`(`identity`: PublicIdentity, x)\n     suspend fun `create`(`signer`: Signer, x)";
        let hidden = hide_client_factories(kotlin, Language::Kotlin)?;
        assert_eq!(hidden.matches("internal suspend fun").count(), 2);
        assert!(hide_client_factories("changed", Language::Kotlin).is_err());
    }

    // The generated identity route leaves the public protocol and class API.
    #[xmtp_common::test(unwrap_try = true)]
    fn identity_routes_are_private_to_the_runtime() {
        let mut swift = String::new();
        let mut kotlin = String::new();
        for name in IDENTITY_ROUTES {
            swift.push_str(&format!(
                "protocol P {{\n    func {name}(members: [PublicIdentity]) async throws\n}}\nopen func {name}(members: [PublicIdentity])async throws {{\n}}\n"
            ));
            kotlin.push_str(&format!(
                "interface I {{\n    suspend fun `{name}`(`members`: List<PublicIdentity>)\n}}\n    override suspend fun `{name}`(`members`: List<PublicIdentity>) {{\n}}\n"
            ));
        }
        let hidden = hide_identity_routes(&swift, Language::Swift)?;
        assert_eq!(hidden.matches("\n    func ").count(), 0);
        assert_eq!(hidden.matches("open func").count(), 0);
        assert_eq!(hidden.matches("\nfunc ").count(), IDENTITY_ROUTES.len());
        let hidden = hide_identity_routes(&kotlin, Language::Kotlin)?;
        assert_eq!(hidden.matches("\n    suspend fun").count(), 0);
        assert_eq!(hidden.matches("override").count(), 0);
        assert_eq!(
            hidden.matches("internal suspend fun").count(),
            IDENTITY_ROUTES.len()
        );
        assert!(hide_identity_routes("changed", Language::Swift).is_err());
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn different_return_type_is_not_forwarded() {
        let group = method("Group", "state", Some(Type::String));
        let dm = method("Dm", "state", Some(Type::Bytes));
        assert!(!same_signature(&group, &dm));
    }
}
