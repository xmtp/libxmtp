//! Shared public values and objects above the private target binding.

mod errors;
mod events;
mod identity;
mod objects;
mod policy;
mod values;

use std::{fmt::Write as _, fs};

use anyhow::Result;
use camino::Utf8Path;
use heck::ToLowerCamelCase;
use uniffi_meta::{FnParamMetadata, Metadata, MetadataGroupMap, Type};

/// The generated tree that receives the projection.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Target {
    /// Node: the binding runs in this process, so static constructors and
    /// top-level functions can call it directly.
    Node,
    /// Browser: the binding runs in the package worker, so constructors and
    /// functions go through the worker proxies.
    Browser,
    /// The browser package's main-thread pure module: public values, codecs,
    /// and synchronous functions over its own binding. It has no objects.
    Pure,
}

#[cfg(test)]
pub(crate) fn public_api_for_test(items: &[&Metadata], target: Target) -> String {
    objects::public_api(items, target)
}

#[cfg(test)]
pub(crate) fn client_members_for_test(items: &[&Metadata]) -> Result<String> {
    let mut code = String::new();
    objects::client_members(&mut code, items)?;
    Ok(code)
}

pub(crate) fn generate(groups: &MetadataGroupMap, out: &Utf8Path, target: Target) -> Result<()> {
    let items = groups
        .values()
        .flat_map(|group| &group.items)
        .collect::<Vec<_>>();
    let events = events::Events::new(&items)?;
    let mut code = String::from("import * as B from '#xmtp/binding';\n");
    if target != Target::Pure {
        code.push_str("import type { Message as BoundMessage } from './runtime/message.js';\n");
    }
    code.push_str(
        "import { Timestamp } from './runtime/ids.js';\nexport { Timestamp };\nexport const objectBrand: unique symbol = Symbol(\"xmtp.object\");\n",
    );
    // The host Message class is the public message on both object targets.
    if target != Target::Pure {
        code.push_str(
            "import type { Message } from './runtime/public/message.js';\nexport type { Message };\n",
        );
    }
    if target == Target::Browser {
        // The binding runs in the package worker: constructors and functions
        // go through its proxies, and storage admin through its template.
        code.push_str("import * as P from './proxy.gen.js';\nimport { createInWorker, initLoggingInWorker } from './package-session.gen.js';\nimport { openStorageAdmin, type StorageAdmin } from './storage-admin.gen.js';\nimport { BridgeError } from './runtime/bridge/wire.js';\n");
    }
    if target != Target::Pure {
        code.push_str("import type { ContentCodec } from './runtime/public/codec.js';\nimport { contentForSend } from './runtime/public/codec-policy.js';\n");
    }
    if target == Target::Pure {
        code.push_str(objects::PURE_PROJECTION);
    } else {
        code.push_str(objects::MEMBERSHIP_GUARDS);
        code.push_str(objects::PROJECTION_INSTALL);
    }
    code.push_str(errors::public_error(target));
    if target != Target::Pure {
        code.push_str(policy::DELIVERY_CURSOR);
    }
    for item in &items {
        match item {
            Metadata::Object(value) if value.imp.has_struct() && value.name == "Client" => {
                objects::client_members(&mut code, &items)?;
            }
            Metadata::Object(value) if value.imp.has_struct() => {
                if !objects::template_object(&value.name, target) {
                    objects::object(&mut code, &items, &value.name, target)?;
                }
            }
            Metadata::Object(value) if value.imp.has_callback_interface() => {
                foreign(&mut code, &items, &value.name)?;
            }
            Metadata::CallbackInterface(value) => foreign(&mut code, &items, &value.name)?,
            Metadata::Record(value) if value.name == "BackendOptions" => {
                code.push_str(policy::BACKEND_OPTIONS)
            }
            Metadata::Record(value) => values::record(&mut code, value, &events)?,
            Metadata::Enum(value) if value.name == "BackendSource" => {
                code.push_str(policy::BACKEND_SOURCE)
            }
            Metadata::Enum(value) if value.name == "StorageLocation" => {
                code.push_str(policy::STORAGE_LOCATION)
            }
            Metadata::Enum(value) if value.name == "Conversation" => {
                code.push_str(policy::CONVERSATION)
            }
            Metadata::Enum(value) if errors::is_details_error(value) => {
                errors::error_class(&mut code, value, target)?;
                if target == Target::Browser && value.name == "XmtpError" {
                    errors::bridge_error(&mut code, value)?;
                }
            }
            Metadata::Enum(value) => values::enumeration(&mut code, value, &events)?,
            Metadata::CustomType(value) if value.name != "Message" && value.name != "Timestamp" => {
                writeln!(
                    code,
                    "export type {} = {};",
                    value.name,
                    public_type(&value.builtin)
                )?;
            }
            Metadata::Func(value) => objects::function(&mut code, value, target)?,
            _ => {}
        }
    }
    if target != Target::Pure {
        objects::projection(&mut code, &items, target)?;
    }
    // A hoisted helper, emitted only when a lift uses it.
    if code.contains("checkedVariant(") {
        code.push_str("function checkedVariant(value: unknown, expected: string | number): void { if (value !== expected) throw new TypeError(\"invalid public enum\"); }\n");
    }
    let path = out.join("public-values.gen.ts");
    fs::write(
        &path,
        crate::format::typescript("public-values.gen.ts", &code)?,
    )?;
    let api = objects::public_api(&items, target);
    fs::write(
        out.join("index.ts"),
        crate::format::typescript("index.ts", &api)?,
    )?;
    // Keep the projection's target import private. Package staging supplies the
    // final browser/node conditions when the public adapters are installed.
    // The browser trees are ES modules only. The pure root re-exports the
    // binding with a star export, which a CommonJS load would drop, and the
    // worker root imports the pure module's classes, so both trees must load
    // as one module kind for `instanceof` to hold under a TypeScript loader.
    let module_type = if target != Target::Node {
        "\"type\":\"module\","
    } else {
        ""
    };
    fs::write(
        out.join("package.json"),
        format!(
            "{{\"private\":true,{module_type}\"imports\":{{\"#xmtp/binding\":\"./xmtp_sdk.ts\"}}}}\n"
        ),
    )?;
    Ok(())
}

fn foreign(code: &mut String, items: &[&Metadata], name: &str) -> Result<()> {
    let methods = items
        .iter()
        .filter_map(|item| match item {
            Metadata::TraitMethod(method) if method.trait_name == name => Some(method),
            _ => None,
        })
        .collect::<Vec<_>>();
    writeln!(code, "export interface {name} {{")?;
    for method in &methods {
        writeln!(
            code,
            "{}({}): {};",
            camel(&method.name),
            parameters(&method.inputs),
            result_type(method.return_type.as_ref(), method.is_async)
        )?;
    }
    code.push_str("}\n");
    for lower in [false, true] {
        let (direction, source, target) = if lower {
            ("lower", name.to_owned(), format!("B.{name}"))
        } else {
            ("lift", format!("B.{name}"), name.to_owned())
        };
        writeln!(
            code,
            "export function {direction}{name}(value: {source}, projection: ObjectProjection): {target} {{ void projection; return {{"
        )?;
        for method in &methods {
            let args = method
                .inputs
                .iter()
                .map(|arg| {
                    format!(
                        "{}: {}",
                        camel(&arg.name),
                        if lower {
                            raw_type(&arg.ty)
                        } else {
                            public_type(&arg.ty)
                        }
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            let passed = method
                .inputs
                .iter()
                .map(|arg| convert(&arg.ty, &camel(&arg.name), !lower))
                .collect::<Vec<_>>()
                .join(", ");
            let await_ = if method.is_async { "await " } else { "" };
            let call = format!("{await_}value.{}({passed})", camel(&method.name));
            let body = match &method.return_type {
                Some(ty) => format!(
                    "const result = {call}; return {};",
                    convert(ty, "result", lower)
                ),
                None => format!("{call};"),
            };
            writeln!(
                code,
                "{}{}({args}) {{ {body} }},",
                if method.is_async { "async " } else { "" },
                camel(&method.name)
            )?;
        }
        code.push_str("}; }\n");
    }
    Ok(())
}

fn camel(name: &str) -> String {
    name.to_lower_camel_case()
}

/// The binding's spelling of a method, function, or parameter name. The
/// TypeScript backend adds `_` to a reserved word, for example `delete_`.
fn identifier(name: &str) -> String {
    const RESERVED: &[&str] = &[
        "await",
        "break",
        "case",
        "catch",
        "class",
        "const",
        "continue",
        "debugger",
        "default",
        "delete",
        "do",
        "else",
        "enum",
        "export",
        "extends",
        "false",
        "finally",
        "for",
        "function",
        "if",
        "implements",
        "import",
        "in",
        "instanceof",
        "interface",
        "let",
        "new",
        "null",
        "package",
        "private",
        "protected",
        "public",
        "return",
        "static",
        "super",
        "switch",
        "this",
        "throw",
        "true",
        "try",
        "typeof",
        "var",
        "void",
        "while",
        "with",
        "yield",
    ];
    let name = camel(name);
    if RESERVED.contains(&name.as_str()) {
        format!("{name}_")
    } else {
        name
    }
}

/// Names of trailing parameters that a caller can omit.
fn optional_parameters(inputs: &[FnParamMetadata]) -> std::collections::BTreeSet<String> {
    super::bridge::none_defaults(inputs)
}

fn parameters(inputs: &[FnParamMetadata]) -> String {
    parameters_with(inputs, &optional_parameters(inputs))
}

fn parameters_with(
    inputs: &[FnParamMetadata],
    defaults: &std::collections::BTreeSet<String>,
) -> String {
    inputs
        .iter()
        .map(|p| {
            let name = identifier(&p.name);
            let optional = if defaults.contains(&name) { "?" } else { "" };
            let ty = match &p.ty {
                Type::Optional { inner_type } if defaults.contains(&name) => inner_type,
                ty => ty,
            };
            format!("{name}{optional}: {}", public_type(ty))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn result_type(ty: Option<&Type>, asynchronous: bool) -> String {
    let ty = ty.map(public_type).unwrap_or_else(|| "void".into());
    if asynchronous {
        format!("Promise<{ty}>")
    } else {
        ty
    }
}

fn public_type(ty: &Type) -> String {
    match ty {
        Type::UInt8
        | Type::Int8
        | Type::UInt16
        | Type::Int16
        | Type::UInt32
        | Type::Int32
        | Type::Float32
        | Type::Float64
        | Type::Duration => "number".into(),
        Type::UInt64 | Type::Int64 => "bigint".into(),
        Type::Boolean => "boolean".into(),
        Type::String => "string".into(),
        Type::Bytes => "Uint8Array".into(),
        Type::Timestamp => "Date".into(),
        Type::Object { name, .. }
        | Type::CallbackInterface { name, .. }
        | Type::Record { name, .. }
        | Type::Enum { name, .. }
        | Type::Custom { name, .. } => name.clone(),
        Type::Box { inner_type } => public_type(inner_type),
        Type::Optional { inner_type } => format!("{} | undefined", public_type(inner_type)),
        Type::Sequence { inner_type } => format!("Array<{}>", public_type(inner_type)),
        Type::Set { inner_type } => format!("Set<{}>", public_type(inner_type)),
        Type::Map {
            key_type,
            value_type,
        } => format!(
            "Map<{}, {}>",
            public_type(key_type),
            public_type(value_type)
        ),
    }
}

fn raw_type(ty: &Type) -> String {
    match ty {
        Type::Bytes => "ArrayBuffer".into(),
        Type::Object { name, imp, .. } if imp.has_struct() => format!("B.{name}Like"),
        Type::Object { name, .. }
        | Type::CallbackInterface { name, .. }
        | Type::Record { name, .. }
        | Type::Enum { name, .. }
        | Type::Custom { name, .. } => format!("B.{name}"),
        Type::Box { inner_type } => raw_type(inner_type),
        Type::Optional { inner_type } => format!("{} | undefined", raw_type(inner_type)),
        Type::Sequence { inner_type } => format!("Array<{}>", raw_type(inner_type)),
        Type::Set { inner_type } => format!("Set<{}>", raw_type(inner_type)),
        Type::Map {
            key_type,
            value_type,
        } => format!("Map<{}, {}>", raw_type(key_type), raw_type(value_type)),
        Type::UInt8
        | Type::Int8
        | Type::UInt16
        | Type::Int16
        | Type::UInt32
        | Type::Int32
        | Type::Float32
        | Type::Float64
        | Type::Duration
        | Type::UInt64
        | Type::Int64
        | Type::Boolean
        | Type::String
        | Type::Timestamp => public_type(ty),
    }
}

fn convert(ty: &Type, value: &str, lower: bool) -> String {
    let direction = if lower { "lower" } else { "lift" };
    match ty {
        Type::Bytes => {
            if lower {
                format!("Uint8Array.from({value}).buffer")
            } else {
                format!("new Uint8Array({value})")
            }
        }
        Type::Object { name, imp, .. } if imp.has_struct() => {
            format!("projection.{direction}{name}({value})")
        }
        Type::Custom { name, .. } if name == "Message" => {
            format!("projection.{direction}Message({value})")
        }
        Type::Record { name, .. }
        | Type::Enum { name, .. }
        | Type::Object { name, .. }
        | Type::CallbackInterface { name, .. } => format!("{direction}{name}({value}, projection)"),
        Type::Box { inner_type } => convert(inner_type, value, lower),
        // A value whose parts need no conversion passes through unchanged.
        Type::Optional { inner_type } => match convert(inner_type, value, lower) {
            inner if inner == value => inner,
            inner => format!("{value} === undefined ? undefined : {inner}"),
        },
        Type::Sequence { inner_type } => match convert(inner_type, "item", lower) {
            inner if inner == "item" => value.into(),
            inner => format!("{value}.map((item) => ({inner}))"),
        },
        Type::Set { inner_type } => match convert(inner_type, "item", lower) {
            inner if inner == "item" => value.into(),
            inner => format!("new Set([...{value}].map((item) => ({inner})))"),
        },
        Type::Map {
            key_type,
            value_type,
        } => match (
            convert(key_type, "key", lower),
            convert(value_type, "item", lower),
        ) {
            (key, item) if key == "key" && item == "item" => value.into(),
            (key, item) => format!("new Map([...{value}].map(([key, item]) => [{key}, {item}]))"),
        },
        Type::UInt8
        | Type::Int8
        | Type::UInt16
        | Type::Int16
        | Type::UInt32
        | Type::Int32
        | Type::Float32
        | Type::Float64
        | Type::Duration
        | Type::UInt64
        | Type::Int64
        | Type::Boolean
        | Type::String
        | Type::Timestamp
        | Type::Custom { .. } => value.into(),
    }
}
