//! Public object classes, the object projection, and public functions.
//!
//! Each public object keeps its binding object in a module-private map. Its
//! methods lower their inputs and lift their results through the value pass.
//! One binding object always lifts to the same public object.

use std::{collections::BTreeSet, fmt::Write as _};

use anyhow::{Result, bail};
use heck::ToLowerCamelCase;
use uniffi_meta::{FnMetadata, FnParamMetadata, Metadata, MethodMetadata, Type};

use super::identity::{ROUTES, is_nullable};
use super::policy::cursor_type;
use super::{
    Target, convert, identifier as camel, optional_parameters, parameters_with, public_type,
};
use crate::client_statics::{self, ClientStatic};

/// The host Client owns these members; the generated members exclude them.
const HOST_CLIENT_MEMBERS: &[&str] = &[
    "clientKey",
    "end",
    "events",
    "startListener",
    "stopListener",
];

/// Functions that the host runtime replaces.
const HOST_FUNCTIONS: &[&str] = &["setLogSink"];

// The pinned uint64 serializer wraps out-of-range values. Check this public
// nonce before lowering it so the canonical Rust calculation gets the exact u64.
const INBOX_NONCE_GUARD: &str = r#"
if (nonce !== undefined && (typeof nonce !== "bigint" || nonce < 0n || nonce > 18446744073709551615n))
  throw new XmtpError.InvalidArgument({ code: "InvalidArgument", category: "input", retryable: false, message: "nonce must be an unsigned 64-bit bigint" });
"#;

/// Guards that route one membership parameter. An empty list uses inbox IDs.
/// A list that mixes inbox IDs and account identities fails before any call.
pub(super) const MEMBERSHIP_GUARDS: &str = r#"
function identityMembers<I, P>(value: Array<I> | Array<P>): value is Array<P> {
  const items: ReadonlyArray<unknown> = value;
  const identities = items.filter((item) => typeof item !== "string").length;
  if (identities !== 0 && identities !== items.length)
    throw new XmtpError.InvalidArgument({ code: "InvalidArgument", category: "input", retryable: false, message: "a member list mixes inbox IDs and account identities" });
  return identities !== 0;
}
function identityMember<I, P>(value: I | P): value is P {
  return typeof value !== "string";
}
"#;

/// The pure module has no objects, so its projection is fixed and needs no
/// host runtime.
pub(super) const PURE_PROJECTION: &str = r#"
/** The pure module has no objects; its projection carries no state. */
export class ObjectProjection {}
const projection = new ObjectProjection();
export function currentProjection(): ObjectProjection { return projection; }
"#;

pub(super) const PROJECTION_INSTALL: &str = r#"
let installed: ObjectProjection | undefined;
/** The host runtime installs its projection once, when its entry loads. */
export function installProjection(value: ObjectProjection): void { installed = value; }
export function currentProjection(): ObjectProjection {
  if (installed === undefined) throw new TypeError("the XMTP public projection is not installed");
  return installed;
}
"#;

/// The browser public layer calls functions in the package worker, so it has
/// only the asynchronous ones; the synchronous ones belong to the pure module.
fn exported_function(function: &FnMetadata, target: Target) -> bool {
    !crate::markers::has(function.docstring.as_deref(), crate::markers::INTERNAL)
        && (target != Target::Browser || function.is_async)
}

/// Objects that the browser public layer takes from its package templates.
pub(super) fn template_object(name: &str, target: Target) -> bool {
    target == Target::Browser && name == "StorageAdmin"
}

fn is_identity_route(owner: &str, method: &str) -> bool {
    ROUTES
        .iter()
        .any(|route| route.owner == owner && route.identity == method)
}

fn struct_objects<'a>(items: &[&'a Metadata], target: Target) -> Vec<&'a str> {
    items
        .iter()
        .filter_map(|item| match item {
            Metadata::Object(value)
                if value.imp.has_struct()
                    && value.name != "Client"
                    && !template_object(&value.name, target) =>
            {
                Some(value.name.as_str())
            }
            _ => None,
        })
        .collect()
}

fn methods<'a>(items: &[&'a Metadata], owner: &str) -> Vec<&'a MethodMetadata> {
    let mut methods = items
        .iter()
        .filter_map(|item| match item {
            Metadata::Method(method)
                if method.self_name == owner
                    && !crate::markers::has(
                        method.docstring.as_deref(),
                        crate::markers::INTERNAL,
                    ) =>
            {
                Some(method)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    methods.sort_by(|left, right| left.name.cmp(&right.name));
    methods
}

/// One call: its public parameters, the binding arguments, and the result.
struct Call {
    parameters: String,
    arguments: String,
    /// A membership union: the guard that selects the identity method, that
    /// method, and its arguments. `arguments` then serve the inbox method.
    routed: Option<Routed>,
    result_type: String,
    result: Option<String>,
}

struct Routed {
    guard: String,
    identity: &'static str,
    arguments: String,
}

impl Call {
    fn uses_projection(&self) -> bool {
        self.arguments.contains("projection")
            || self
                .routed
                .as_ref()
                .is_some_and(|routed| routed.arguments.contains("projection"))
            || self
                .result
                .as_deref()
                .is_some_and(|result| result.contains("projection"))
    }
}

fn call(
    owner: &str,
    name: &str,
    inputs: &[FnParamMetadata],
    output: Option<&Type>,
    asynchronous: bool,
) -> Call {
    let mut defaults = optional_parameters(inputs);
    // The binding reader interfaces keep their options optional.
    if name == "messageReader" {
        defaults.insert("options".into());
    }
    let route = ROUTES
        .iter()
        .find(|route| route.owner == owner && route.method == name);
    let mut parameters = parameters_with(inputs, &defaults);
    let arguments = inputs
        .iter()
        .map(|input| convert(&input.ty, &camel(&input.name), true))
        .collect::<Vec<_>>()
        .join(", ");
    // A membership union calls the binding identity method with lowered
    // identities, and the inbox method otherwise.
    let routed = route.map(|route| {
        let member = route.member;
        let arguments = inputs
            .iter()
            .map(|input| {
                let input_name = camel(&input.name);
                if input_name != member {
                    convert(&input.ty, &input_name, true)
                } else if route.list {
                    format!("{member}.map((item) => lowerPublicIdentity(item, projection))")
                } else {
                    format!("lowerPublicIdentity({member}, projection)")
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        let guard = if route.list {
            format!("identityMembers<InboxId, PublicIdentity>({member})")
        } else {
            format!("identityMember<InboxId, PublicIdentity>({member})")
        };
        Routed {
            guard,
            identity: route.identity,
            arguments,
        }
    });
    if let Some(route) = route {
        let (inbox, union) = if route.list {
            ("Array<InboxId>", "Array<InboxId> | Array<PublicIdentity>")
        } else {
            ("InboxId", "InboxId | PublicIdentity")
        };
        parameters = parameters.replacen(
            &format!("{}: {inbox}", route.member),
            &format!("{}: {union}", route.member),
            1,
        );
    }
    let nullable = is_nullable(owner, name);
    let (result_type, result) = match output {
        None => ("void".to_owned(), None),
        Some(Type::Optional { inner_type }) if nullable => (
            format!("{} | null", public_type(inner_type)),
            Some(match convert(inner_type, "result", false) {
                inner if inner == "result" => "result ?? null".to_owned(),
                inner => format!("result === undefined || result === null ? null : {inner}"),
            }),
        ),
        Some(ty) => (public_type(ty), Some(convert(ty, "result", false))),
    };
    let result_type = cursor_type(owner, name, result_type);
    let result_type = if asynchronous {
        format!("Promise<{result_type}>")
    } else {
        result_type
    };
    Call {
        parameters,
        arguments,
        routed,
        result_type,
        result,
    }
}

fn render_body(
    code: &mut String,
    call: &Call,
    callee: &dyn Fn(&str) -> String,
    asynchronous: bool,
) -> Result<()> {
    let await_ = if asynchronous { "await " } else { "" };
    let uses_projection = call.uses_projection();
    // Every call converts a thrown value, including a call whose binding
    // cannot fail: a browser proxy can refuse any call of an ended client,
    // and the package worker can fail under any call (P8).
    code.push_str("try {\n");
    if uses_projection {
        code.push_str("const projection = currentProjection();\n");
    }
    match &call.result {
        Some(result) => writeln!(
            code,
            "const result = {await_}{};\nreturn {result};",
            callee(&call.arguments)
        )?,
        None => writeln!(code, "{await_}{};", callee(&call.arguments))?,
    }
    code.push_str("} catch (error) {\nthrow publicError(error);\n}\n");
    Ok(())
}

/// The Group and Dm sends that take a typed codec and its value as well as an
/// envelope (Decision 23).
fn is_codec_send(owner: &str, name: &str) -> bool {
    matches!(owner, "Group" | "Dm") && matches!(name, "send" | "prepareMessage")
}

/// A synchronous, argument-free, infallible member with a result.
pub(super) fn is_getter(method: &MethodMetadata) -> bool {
    !method.is_async
        && method.inputs.is_empty()
        && method.throws.is_none()
        && method.return_type.is_some()
}

fn member(code: &mut String, owner: &str, method: &MethodMetadata, receiver: &str) -> Result<()> {
    let name = camel(&method.name);
    let call = call(
        owner,
        &name,
        &method.inputs,
        method.return_type.as_ref(),
        method.is_async,
    );
    // Decision 14: a synchronous, argument-free, infallible member is a
    // readonly getter.
    let getter = is_getter(method);
    if is_codec_send(owner, &name) {
        // Decision 23: a typed codec form overloads the envelope form. The
        // codec steps run first, so a failed step makes no binding call.
        writeln!(
            code,
            "{name}(encoded: EncodedContent, options?: SendOptions): {result};\n{name}<T>(codec: ContentCodec<T>, value: NoInfer<T>, options?: SendOptions): {result};\nasync {name}<T>(content: EncodedContent | ContentCodec<T>, valueOrOptions?: T | SendOptions, sendOptions?: SendOptions): {result} {{\nconst [encoded, options] = contentForSend(content, valueOrOptions, sendOptions);",
            result = call.result_type
        )?;
    } else {
        writeln!(
            code,
            "{}{}{name}({}): {} {{",
            if method.is_async { "async " } else { "" },
            if getter { "get " } else { "" },
            call.parameters,
            call.result_type
        )?;
    }
    let routed = call.routed.as_ref();
    render_body(
        code,
        &call,
        &|args| match routed {
            Some(routed) => format!(
                "({} ? {receiver}.{}({}) : {receiver}.{name}({args}))",
                routed.guard, routed.identity, routed.arguments
            ),
            None => format!("{receiver}.{name}({args})"),
        },
        method.is_async,
    )?;
    code.push_str("}\n");
    Ok(())
}

pub(super) fn object(
    code: &mut String,
    items: &[&Metadata],
    name: &str,
    target: Target,
) -> Result<()> {
    let key = name.to_lower_camel_case();
    writeln!(
        code,
        "const {key}Bindings = new WeakMap<{name}, B.{name}Like>();\nconst {key}Objects = new WeakMap<B.{name}Like, {name}>();\nlet new{name}!: () => {name};"
    )?;
    writeln!(
        code,
        "export class {name} {{\ndeclare readonly [objectBrand]: \"{name}\";\nstatic {{ new{name} = () => new {name}(); }}\nprivate constructor() {{}}"
    )?;
    // Browser: the package storage admin opens without a Client.
    if target == Target::Browser && name == "Storage" {
        // Every admin member, and the open, throws public errors (P8).
        code.push_str("/** Open a lease on the package storage worker. It needs no Client. */\nstatic admin(): Promise<StorageAdmin> { return openStorageAdmin(publicError); }\n");
    }
    for item in items {
        if let Metadata::Constructor(constructor) = item
            && constructor.self_name == name
        {
            let method_name = camel(&constructor.name);
            let mut call = call(
                name,
                &method_name,
                &constructor.inputs,
                None,
                constructor.is_async,
            );
            call.result = Some(format!("projection.lift{name}(result)"));
            call.result_type = if constructor.is_async {
                format!("Promise<{name}>")
            } else {
                name.to_owned()
            };
            writeln!(
                code,
                "static {}{method_name}({}): {} {{",
                if constructor.is_async { "async " } else { "" },
                call.parameters,
                call.result_type
            )?;
            // Browser constructors run in the package worker.
            let callee = |args: &str| match target {
                Target::Node | Target::Pure => format!("B.{name}.{method_name}({args})"),
                Target::Browser => {
                    format!("createInWorker((session) => P.{name}.{method_name}(session, {args}))")
                }
            };
            render_body(code, &call, &callee, constructor.is_async)?;
            code.push_str("}\n");
        }
    }
    match name {
        "Conversations" => code.push_str("stream(options?: ConversationStreamOptions): ConversationStream { return openConversationStream(this, options); }\nstreamAllMessages(options?: MessageStreamOptions): MessageStream { return openAllMessages(this, options); }\n"),
        "Group" | "Dm" => writeln!(code, "streamMessages(options?: ConversationMessageStreamOptions): MessageStream {{ return open{name}Messages(this, options); }}")?,
        _ => {}
    }
    for method in methods(items, name) {
        if is_identity_route(name, &camel(&method.name)) {
            continue;
        }
        member(code, name, method, &format!("unwrap{name}(this)"))?;
    }
    code.push_str("}\n");
    writeln!(
        code,
        "export function wrap{name}(value: B.{name}Like): {name} {{\n  const known = {key}Objects.get(value);\n  if (known !== undefined) return known;\n  const wrapper = new{name}();\n  {key}Bindings.set(wrapper, value);\n  {key}Objects.set(value, wrapper);\n  return wrapper;\n}}\nexport function unwrap{name}(value: {name}): B.{name}Like {{\n  const binding = {key}Bindings.get(value);\n  if (binding === undefined) throw new TypeError(\"not an XMTP {name}\");\n  return binding;\n}}"
    )?;
    Ok(())
}

pub(super) fn client_members(code: &mut String, items: &[&Metadata]) -> Result<()> {
    // The binding stays in a module-private map, as for the object classes.
    code.push_str("import type { ClientBinding } from \"./client-forwarding.gen.js\";\nconst clientBindings = new WeakMap<ClientMembers, ClientBinding>();\n/** Attach the binding Client when the host creates a public Client. */\nexport function attachClientBinding(client: ClientMembers, binding: ClientBinding): void { clientBindings.set(client, binding); }\nexport function clientBinding(client: ClientMembers): ClientBinding {\n  const binding = clientBindings.get(client);\n  if (binding === undefined) throw new TypeError(\"not an XMTP Client\");\n  return binding;\n}\n/** Generated public Client members. The host Client supplies its binding. */\nexport abstract class ClientMembers {\n");
    for method in methods(items, "Client") {
        if HOST_CLIENT_MEMBERS.contains(&camel(&method.name).as_str()) {
            continue;
        }
        member(code, "Client", method, "clientBinding(this)")?;
    }
    // The public Client inherits the statics.
    for item in client_statics::client_statics(items)? {
        client_static(code, &item)?;
    }
    code.push_str("}\n");
    Ok(())
}

/// A Client static calls the public function that this module exports for
/// the same `#[sdk_export(client_static)]` function.
fn client_static(code: &mut String, item: &ClientStatic) -> Result<()> {
    let function = camel(&item.function.name);
    let parameters = item
        .parameters
        .iter()
        .map(|parameter| (*parameter).clone())
        .collect::<Vec<_>>();
    // A parameter that the public function lets callers leave out stays
    // optional; `client_statics` keeps it after every required parameter.
    let declared = parameters_with(&parameters, &optional_parameters(&item.function.inputs));
    let result = call(
        "",
        &function,
        &item.function.inputs,
        item.function.return_type.as_ref(),
        true,
    )
    .result_type;
    let arguments = item
        .function
        .inputs
        .iter()
        .map(|input| camel(&input.name))
        .collect::<Vec<_>>()
        .join(", ");
    writeln!(
        code,
        "static {}({declared}): {result} {{\nreturn {function}({arguments});\n}}",
        item.name
    )?;
    Ok(())
}

pub(super) fn function(code: &mut String, function: &FnMetadata, target: Target) -> Result<()> {
    let name = camel(&function.name);
    if HOST_FUNCTIONS.contains(&name.as_str()) || !exported_function(function, target) {
        return Ok(());
    }
    let asynchronous = function.is_async;
    let call = call(
        "",
        &name,
        &function.inputs,
        function.return_type.as_ref(),
        asynchronous,
    );
    writeln!(
        code,
        "export {}function {name}({}): {} {{",
        if asynchronous { "async " } else { "" },
        call.parameters,
        call.result_type
    )?;
    let callee = |args: &str| match target {
        Target::Node | Target::Pure => format!("B.{name}({args})"),
        Target::Browser => {
            if name == "initLogging" {
                format!(
                    "initLoggingInWorker({args}, (session, options) => P.{name}(session, options))"
                )
            } else {
                format!("createInWorker((session) => P.{name}(session, {args}))")
            }
        }
    };
    if name == "generateInboxId" {
        if asynchronous
            || !matches!(function.inputs.as_slice(), [identity, nonce]
            if identity.name == "identity" && matches!(&identity.ty, Type::Record { name, .. } if name == "PublicIdentity")
                && nonce.name == "nonce" && matches!(&nonce.ty, Type::Optional { inner_type } if matches!(inner_type.as_ref(), Type::UInt64)))
        {
            bail!("generateInboxId: expected a pure identity and optional u64 nonce");
        }
        code.push_str(INBOX_NONCE_GUARD);
    }
    render_body(code, &call, &callee, asynchronous)?;
    code.push_str("}\n");
    Ok(())
}

/// The projection that the value pass calls for objects and messages. The
/// host runtime supplies the Message conversion.
pub(super) fn projection(code: &mut String, items: &[&Metadata], target: Target) -> Result<()> {
    code.push_str("export abstract class ObjectProjection {\nabstract liftMessage(value: BoundMessage): Message;\nabstract lowerMessage(value: Message): BoundMessage;\nisBackend(value: BackendSource): value is Backend { return value instanceof Backend; }\n");
    for name in struct_objects(items, target) {
        writeln!(
            code,
            "lift{name}(value: B.{name}Like): {name} {{ return wrap{name}(value); }}\nlower{name}(value: {name}): B.{name}Like {{ return unwrap{name}(value); }}"
        )?;
    }
    code.push_str("}\n");
    Ok(())
}

/// The package root and the names it exports. Internal conversion
/// functions, the projection, and the generated Client members stay out.
pub(super) fn public_api(items: &[&Metadata], target: Target) -> String {
    if target == Target::Pure {
        return pure_api(items);
    }
    let mut values = BTreeSet::new();
    let mut types = BTreeSet::new();
    for item in items {
        match item {
            Metadata::Object(value)
                if value.imp.has_struct()
                    && value.name != "Client"
                    && !template_object(&value.name, target) =>
            {
                values.insert(value.name.clone());
            }
            Metadata::Object(value) if value.imp.has_callback_interface() => {
                types.insert(value.name.clone());
            }
            Metadata::CallbackInterface(value) => {
                types.insert(value.name.clone());
            }
            // The host Client takes its options with codecs. Message data
            // carries the internal client key; the host Message replaces it.
            Metadata::Record(value)
                if value.name != "ClientOptions"
                    && value.name != "MessageData"
                    && !crate::markers::has(
                        value.docstring.as_deref(),
                        crate::markers::INTERNAL,
                    ) =>
            {
                types.insert(value.name.clone());
            }
            // Decision 15: the public error class is a value. Callback
            // error enums stay internal.
            Metadata::Enum(value) if super::errors::is_details_error(value) => {
                values.insert(value.name.clone());
            }
            Metadata::Enum(value) if !value.shape.is_error() => {
                types.insert(value.name.clone());
            }
            Metadata::CustomType(value) if value.name != "Message" && value.name != "Timestamp" => {
                types.insert(value.name.clone());
            }
            Metadata::Func(value) => {
                let name = camel(&value.name);
                if !HOST_FUNCTIONS.contains(&name.as_str()) && exported_function(value, target) {
                    values.insert(name);
                }
            }
            _ => {}
        }
    }
    // The runtime adds the final host admission check to the async log sink.
    types.remove("LogSink");
    types.insert("DeliveryCursor".to_owned());
    let join = |names: BTreeSet<String>| names.into_iter().collect::<Vec<_>>().join(", ");
    // Explicit names: Node's CommonJS interop drops a star re-export. The
    // browser has no process log sink or standalone codecs here: the pure
    // module carries the codecs, and F7 adds the browser log sink.
    let target_exports = match target {
        Target::Node => {
            "export { setLogSink, type LogSink } from \"./runtime/public/logging.js\";\nexport { ActionsCodec, AttachmentCodec, DeleteMessageCodec, GroupUpdatedCodec, IntentCodec, LeaveRequestCodec, MarkdownCodec, MultiRemoteAttachmentCodec, ReactionV2Codec, ReadReceiptCodec, RemoteAttachmentCodec, ReplyCodec, TextCodec, TransactionReferenceCodec, WalletSendCallsCodec } from \"./runtime/public/codecs.js\";\n"
        }
        Target::Browser => {
            "export { setLogSink, type LogSink } from \"./runtime/public/logging.js\";\nexport type { StorageAdmin } from \"./storage-admin.gen.js\";\nexport { generateInboxId } from \"../typescript-pure/index.js\";\n"
        }
        Target::Pure => unreachable!("the pure module has its own entry"),
    };
    format!(
        "// The package root, generated from the public projection. Do not edit this\n// output.\nimport \"./runtime/public/projection.js\";\n\nexport {{ Client, type ClientOptions }} from \"./runtime/public/client.js\";\nexport {{ Message }} from \"./runtime/public/message.js\";\nexport type {{ AnyContentCodec, ContentCodec }} from \"./runtime/public/codec.js\";\nexport {{ Timestamp }} from \"./runtime/ids.js\";\nexport {{ ConversationStream, MessageStream, type StreamCloseReason, type StreamOptions, type ConversationStreamOptions, type MessageStreamOptions, type ConversationMessageStreamOptions }} from \"./runtime/public/streams.js\";\nexport {{ EventStream }} from \"./runtime/public/events.js\";\n{target_exports}export {{ {} }} from \"./public-values.gen.js\";\nexport type {{ {} }} from \"./public-values.gen.js\";\n",
        join(values),
        join(types)
    )
}

/// The pure module's public entry: the public values and functions of its
/// binding, the standalone codecs, and the WASM loader. Every shared name has
/// the Node public declaration.
fn pure_api(items: &[&Metadata]) -> String {
    let mut values = BTreeSet::new();
    let mut types = BTreeSet::new();
    for item in items {
        match item {
            Metadata::Record(value) => {
                types.insert(value.name.clone());
            }
            Metadata::Enum(value) if super::errors::is_details_error(value) => {
                values.insert(value.name.clone());
            }
            Metadata::Enum(value) if !value.shape.is_error() => {
                types.insert(value.name.clone());
            }
            Metadata::CustomType(value) if value.name != "Timestamp" => {
                types.insert(value.name.clone());
            }
            Metadata::Func(value) => {
                values.insert(camel(&value.name));
            }
            _ => {}
        }
    }
    let join = |names: BTreeSet<String>| names.into_iter().collect::<Vec<_>>().join(", ");
    format!(
        "// The pure module's package root, generated from the public projection. Do\n// not edit this output.\nexport {{ initPureWasm }} from \"./binding.js\";\nexport type {{ ContentCodec }} from \"./runtime/public/codec.js\";\nexport {{ Timestamp }} from \"./runtime/ids.js\";\nexport {{ ActionsCodec, AttachmentCodec, DeleteMessageCodec, GroupUpdatedCodec, IntentCodec, LeaveRequestCodec, MarkdownCodec, MultiRemoteAttachmentCodec, ReactionV2Codec, ReadReceiptCodec, RemoteAttachmentCodec, ReplyCodec, TextCodec, TransactionReferenceCodec, WalletSendCallsCodec }} from \"./runtime/public/codecs.js\";\nexport {{ {} }} from \"./public-values.gen.js\";\nexport type {{ {} }} from \"./public-values.gen.js\";\n",
        join(values),
        join(types)
    )
}

#[cfg(test)]
mod pure_inbox_tests {
    use super::*;
    use uniffi_meta::{DefaultValueMetadata, LiteralMetadata};

    #[xmtp_common::test(unwrap_try = true)]
    fn pure_inbox_projection_keeps_sync_optional_nonce_and_excludes_worker_dispatch() {
        let mut nonce = FnParamMetadata::simple(
            "nonce",
            Type::Optional {
                inner_type: Box::new(Type::UInt64),
            },
        );
        nonce.default = Some(DefaultValueMetadata::Literal(LiteralMetadata::None));
        let mut metadata = FnMetadata {
            module_path: "xmtp_sdk::signer".into(),
            name: "generate_inbox_id".into(),
            orig_name: None,
            is_async: false,
            inputs: vec![
                FnParamMetadata::simple(
                    "identity",
                    Type::Record {
                        module_path: "xmtp_sdk::signer".into(),
                        name: "PublicIdentity".into(),
                    },
                ),
                nonce,
            ],
            return_type: Some(Type::Custom {
                module_path: "xmtp_sdk::ids".into(),
                name: "InboxId".into(),
                builtin: Box::new(Type::String),
            }),
            throws: None,
            checksum: None,
            docstring: Some("@xmtp-pure".into()),
        };
        for target in [Target::Node, Target::Pure] {
            let mut code = String::new();
            function(&mut code, &metadata, target)?;
            assert!(
                code.contains("function generateInboxId(identity: PublicIdentity, nonce?: bigint)")
            );
            assert!(code.contains("typeof nonce !== \"bigint\""));
            assert!(code.contains("nonce < 0n"));
            assert!(code.contains("nonce > 18446744073709551615n"));
            assert!(code.contains("B.generateInboxId("));
            assert!(!code.contains("await"));
            assert!(!code.contains("createInWorker"));
        }
        let mut worker = String::new();
        function(&mut worker, &metadata, Target::Browser)?;
        assert!(worker.is_empty());
        let root = public_api(&[], Target::Browser);
        assert!(root.contains("export { generateInboxId } from \"../typescript-pure/index.js\""));
        metadata.inputs[1].ty = Type::Int64;
        assert!(function(&mut String::new(), &metadata, Target::Pure).is_err());
    }
}
