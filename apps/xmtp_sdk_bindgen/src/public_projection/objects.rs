//! Public object classes, the object projection, and public functions.
//!
//! Each public object keeps its binding object in a module-private map. Its
//! methods lower their inputs and lift their results through the value pass.
//! One binding object always lifts to the same public object.

use std::{collections::BTreeSet, fmt::Write as _};

use anyhow::Result;
use heck::ToLowerCamelCase;
use uniffi_meta::{FnMetadata, FnParamMetadata, Metadata, MethodMetadata, Type};

use super::{
    Target, convert, identifier as camel, optional_parameters, parameters_with, public_type,
};
use crate::{identity_unions::ROUTES, nullable_identity::is_nullable};

/// The host Client owns these members; the generated members exclude them.
const HOST_CLIENT_MEMBERS: &[&str] = &[
    "clientKey",
    "end",
    "events",
    "startListener",
    "stopListener",
];

/// Functions that the host runtime replaces.
const HOST_FUNCTIONS: &[&str] = &["setLogSink", "setLogSinkQueued"];

pub(super) const PROJECTION_INSTALL: &str = r#"
let installed: ObjectProjection | undefined;
/** The host runtime installs its projection once, when its entry loads. */
export function installProjection(value: ObjectProjection): void { installed = value; }
export function currentProjection(): ObjectProjection {
  if (installed === undefined) throw new TypeError("the XMTP public projection is not installed");
  return installed;
}
"#;

fn is_identity_route(owner: &str, method: &str) -> bool {
    ROUTES
        .iter()
        .any(|route| route.owner == owner && route.identity == method)
}

fn struct_objects<'a>(items: &[&'a Metadata]) -> Vec<&'a str> {
    items
        .iter()
        .filter_map(|item| match item {
            Metadata::Object(value) if value.imp.has_struct() && value.name != "Client" => {
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
            Metadata::Method(method) if method.self_name == owner => Some(method),
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
    result_type: String,
    result: Option<String>,
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
        .map(|input| {
            let input_name = camel(&input.name);
            match route {
                Some(route) if route.member == input_name && route.list => format!(
                    "identityMembers<InboxId, PublicIdentity>({input_name}) ? {input_name}.map((item) => lowerPublicIdentity(item, projection)) : {input_name}"
                ),
                Some(route) if route.member == input_name => format!(
                    "identityMember<InboxId, PublicIdentity>({input_name}) ? lowerPublicIdentity({input_name}, projection) : {input_name}"
                ),
                _ => convert(&input.ty, &input_name, true),
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
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
    let result_type = if asynchronous {
        format!("Promise<{result_type}>")
    } else {
        result_type
    };
    Call {
        parameters,
        arguments,
        result_type,
        result,
    }
}

fn render_body(code: &mut String, call: &Call, target: &str, asynchronous: bool) -> Result<()> {
    let await_ = if asynchronous { "await " } else { "" };
    let uses_projection = call.arguments.contains("projection")
        || call
            .result
            .as_deref()
            .is_some_and(|result| result.contains("projection"));
    if uses_projection {
        code.push_str("const projection = currentProjection();\n");
    }
    match &call.result {
        Some(result) => writeln!(
            code,
            "const result = {await_}{target}({});\nreturn {result};",
            call.arguments
        )?,
        None => writeln!(code, "{await_}{target}({});", call.arguments)?,
    }
    Ok(())
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
    writeln!(
        code,
        "{}{}{name}({}): {} {{",
        if method.is_async { "async " } else { "" },
        if getter { "get " } else { "" },
        call.parameters,
        call.result_type
    )?;
    render_body(code, &call, &format!("{receiver}.{name}"), method.is_async)?;
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
    if target == Target::Node {
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
                render_body(
                    code,
                    &call,
                    &format!("B.{name}.{method_name}"),
                    constructor.is_async,
                )?;
                code.push_str("}\n");
            }
        }
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
    code.push_str("const clientBindings = new WeakMap<ClientMembers, B.ClientLike>();\n/** Attach the binding Client when the host creates a public Client. */\nexport function attachClientBinding(client: ClientMembers, binding: B.ClientLike): void { clientBindings.set(client, binding); }\nexport function clientBinding(client: ClientMembers): B.ClientLike {\n  const binding = clientBindings.get(client);\n  if (binding === undefined) throw new TypeError(\"not an XMTP Client\");\n  return binding;\n}\n/** Generated public Client members. The host Client supplies its binding. */\nexport abstract class ClientMembers {\n");
    for method in methods(items, "Client") {
        if HOST_CLIENT_MEMBERS.contains(&camel(&method.name).as_str()) {
            continue;
        }
        member(code, "Client", method, "clientBinding(this)")?;
    }
    code.push_str("}\n");
    Ok(())
}

pub(super) fn function(code: &mut String, function: &FnMetadata) -> Result<()> {
    let name = camel(&function.name);
    if HOST_FUNCTIONS.contains(&name.as_str()) {
        return Ok(());
    }
    let call = call(
        "",
        &name,
        &function.inputs,
        function.return_type.as_ref(),
        function.is_async,
    );
    writeln!(
        code,
        "export {}function {name}({}): {} {{",
        if function.is_async { "async " } else { "" },
        call.parameters,
        call.result_type
    )?;
    render_body(code, &call, &format!("B.{name}"), function.is_async)?;
    code.push_str("}\n");
    Ok(())
}

/// The projection that the value pass calls for objects and messages. The
/// host runtime supplies the Message conversion.
pub(super) fn projection(code: &mut String, items: &[&Metadata]) -> Result<()> {
    code.push_str("export abstract class ObjectProjection {\nabstract liftMessage(value: BoundMessage): Message;\nabstract lowerMessage(value: Message): BoundMessage;\nisBackend(value: BackendSource): value is Backend { return value instanceof Backend; }\n");
    for name in struct_objects(items) {
        writeln!(
            code,
            "lift{name}(value: B.{name}Like): {name} {{ return wrap{name}(value); }}\nlower{name}(value: {name}): B.{name}Like {{ return unwrap{name}(value); }}"
        )?;
    }
    code.push_str("}\n");
    Ok(())
}

/// The private public entry and the names it re-exports. Internal conversion
/// functions, the projection, and the generated Client members stay out.
pub(super) fn public_api(items: &[&Metadata]) -> String {
    let mut values = BTreeSet::new();
    let mut types = BTreeSet::new();
    for item in items {
        match item {
            Metadata::Object(value) if value.imp.has_struct() && value.name != "Client" => {
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
                if value.name != "ClientOptions" && value.name != "MessageData" =>
            {
                types.insert(value.name.clone());
            }
            // Thrown errors keep their binding classes until the public error
            // shape is decided.
            Metadata::Enum(value) if !value.shape.is_error() => {
                types.insert(value.name.clone());
            }
            Metadata::CustomType(value) if value.name != "Message" && value.name != "Timestamp" => {
                types.insert(value.name.clone());
            }
            Metadata::Func(value) => {
                let name = camel(&value.name);
                if !HOST_FUNCTIONS.contains(&name.as_str()) {
                    values.insert(name);
                }
            }
            _ => {}
        }
    }
    let join = |names: BTreeSet<String>| names.into_iter().collect::<Vec<_>>().join(", ");
    // Explicit names: Node's CommonJS interop drops a star re-export.
    format!(
        "// The private public entry, generated from the public projection. The\n// package roots re-export it once every target uses it. Do not edit this output.\nimport \"./runtime/public/projection.js\";\n\nexport {{ Client, type ClientOptions }} from \"./runtime/public/client.js\";\nexport {{ Message }} from \"./runtime/public/message.js\";\nexport type {{ AnyContentCodec, ContentCodec }} from \"./runtime/public/codec.js\";\nexport {{ Timestamp }} from \"./runtime/ids.js\";\nexport {{ {} }} from \"./public-values.gen.js\";\nexport type {{ {} }} from \"./public-values.gen.js\";\n",
        join(values),
        join(types)
    )
}
