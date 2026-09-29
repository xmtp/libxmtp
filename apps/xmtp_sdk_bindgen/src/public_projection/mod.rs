//! Shared public values above the private target binding.

mod policy;
mod values;

use std::{fmt::Write as _, fs};

use anyhow::Result;
use camino::Utf8Path;
use heck::ToLowerCamelCase;
use uniffi_meta::{FnParamMetadata, Metadata, MetadataGroupMap, Type};

pub(crate) fn generate(groups: &MetadataGroupMap, out: &Utf8Path) -> Result<()> {
    let items = groups
        .values()
        .flat_map(|group| &group.items)
        .collect::<Vec<_>>();
    let mut code = String::from(
        "import * as B from '#xmtp/binding';\nimport type { Message as BoundMessage } from './runtime/message.js';\nimport { Timestamp } from './runtime/ids.js';\nexport { Timestamp };\nexport const objectBrand: unique symbol = Symbol(\"xmtp.object\");\nfunction checkedVariant(value: unknown, expected: string | number): void { if (value !== expected) throw new TypeError(\"invalid public enum\"); }\n",
    );
    code.push_str("export type Message = Omit<MessageData, 'clientKey'>;\n");
    let mut context = String::from(
        "export abstract class ObjectProjection {\n  abstract isBackend(value: BackendSource): value is Backend;\n  abstract liftMessage(value: BoundMessage): Message;\n  abstract lowerMessage(value: Message): BoundMessage;\n",
    );
    for item in &items {
        match item {
            Metadata::Object(value) if value.imp.has_struct() => {
                let name = &value.name;
                writeln!(
                    context,
                    "abstract lift{name}(value: B.{name}Like): {name};\nabstract lower{name}(value: {name}): B.{name}Like;"
                )?;
                writeln!(
                    code,
                    "export interface {name} {{ readonly [objectBrand]: '{name}';"
                )?;
                for method in &items {
                    if let Metadata::Method(method) = method
                        && method.self_name == *name
                    {
                        let result = result_type(method.return_type.as_ref(), method.is_async);
                        writeln!(
                            code,
                            "{}({}): {result};",
                            camel(&method.name),
                            parameters(&method.inputs)
                        )?;
                    }
                }
                code.push_str("}\n");
            }
            Metadata::Object(value) if value.imp.has_callback_interface() => {
                foreign(&mut code, &items, &value.name)?;
            }
            Metadata::CallbackInterface(value) => foreign(&mut code, &items, &value.name)?,
            Metadata::Record(value) if value.name == "BackendOptions" => {
                code.push_str(policy::BACKEND_OPTIONS)
            }
            Metadata::Record(value) => values::record(&mut code, value)?,
            Metadata::Enum(value) if value.name == "BackendSource" => {
                code.push_str(policy::BACKEND_SOURCE)
            }
            Metadata::Enum(value) if value.name == "StorageLocation" => {
                code.push_str(policy::STORAGE_LOCATION)
            }
            Metadata::Enum(value) => values::enumeration(&mut code, value)?,
            Metadata::CustomType(value) if value.name != "Message" && value.name != "Timestamp" => {
                writeln!(
                    code,
                    "export type {} = {};",
                    value.name,
                    public_type(&value.builtin)
                )?;
            }
            _ => {}
        }
    }
    context.push_str("}\n");
    code.push_str(&context);
    let path = out.join("public-values.gen.ts");
    fs::write(
        &path,
        crate::format::typescript("public-values.gen.ts", &code)?,
    )?;
    // Keep the projection's target import private. Package staging supplies the
    // final browser/node conditions when the public adapters are installed.
    fs::write(
        out.join("package.json"),
        "{\"private\":true,\"imports\":{\"#xmtp/binding\":\"./xmtp_sdk.ts\"}}\n",
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

fn parameters(inputs: &[FnParamMetadata]) -> String {
    let defaults = super::bridge::none_defaults(inputs);
    inputs
        .iter()
        .map(|p| {
            let name = camel(&p.name);
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
        Type::Optional { inner_type } => format!(
            "{value} === undefined ? undefined : {}",
            convert(inner_type, value, lower)
        ),
        Type::Sequence { inner_type } => format!(
            "{value}.map((item) => ({}))",
            convert(inner_type, "item", lower)
        ),
        Type::Set { inner_type } => format!(
            "new Set([...{value}].map((item) => ({})))",
            convert(inner_type, "item", lower)
        ),
        Type::Map {
            key_type,
            value_type,
        } => format!(
            "new Map([...{value}].map(([key, item]) => [{}, {}]))",
            convert(key_type, "key", lower),
            convert(value_type, "item", lower)
        ),
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
