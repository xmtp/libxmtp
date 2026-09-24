use std::{collections::BTreeMap, fmt::Write as _, fs, process::Command};

use anyhow::{Context, Result, bail};
use camino::Utf8Path;
use heck::ToLowerCamelCase;
use sha2::{Digest, Sha256};
use ubrn_bindgen::wasm_metadata;
use uniffi_bindgen::{BindgenLoader, BindgenPaths, GlobalConfig};
use uniffi_meta::{Metadata, MetadataGroupMap, ObjectImpl, TraitKind, Type};

#[derive(Clone)]
struct Operation {
    owner: Option<String>,
    name: String,
    key: String,
    inputs: Vec<(String, Type)>,
    output: Option<Type>,
    constructor: bool,
    immutable: bool,
}

pub(crate) fn generate(lib: &Utf8Path, out: &Utf8Path) -> Result<()> {
    let config = Utf8Path::new("apps/xmtp_sdk_bindgen/uniffi-global.toml");
    let (global, roots) = GlobalConfig::from_file(config)?;
    let mut paths = BindgenPaths::default();
    if let Some(roots) = roots {
        paths.add_layer(roots);
    }
    let crate_root = paths
        .get_crate_root("xmtp_sdk")
        .context("missing xmtp_sdk crate root")?;
    let loader = BindgenLoader::new(paths, global);
    let metadata = loader.load_metadata_specialized(lib, |_path, bytes| {
        if wasm_metadata::looks_like_wasm(bytes) {
            Ok(Some(wasm_metadata::extract_from_wasm_bytes(bytes)?))
        } else {
            Ok(None)
        }
    })?;
    if !wasm_metadata::looks_like_wasm(&fs::read(lib)?) {
        bail!("{}: browser bridge needs the unstaged WASM artifact", lib);
    }
    super::validate::validate_metadata(&metadata)?;
    let items = metadata
        .values()
        .flat_map(|group| group.items.iter().cloned())
        .collect::<Vec<_>>();
    validate_bridge(&items)?;
    let names = super::id_names::typescript_rename_map(&metadata, &crate_root.join("uniffi.toml"))?;
    let operations = operations(&items, &names);
    let hash = contract_hash(&metadata);
    fs::create_dir_all(out)?;
    let generated = render(&items, &operations, &hash, &names)?;
    for (name, body) in &generated {
        fs::write(out.join(name), body)?;
    }
    let formatter = Utf8Path::new("node_modules/.bin/oxfmt");
    if !formatter.exists() {
        bail!("browser bridge formatter missing: run `just install` before SDK generation");
    }
    let status = Command::new(formatter)
        .arg("--config")
        .arg("apps/xmtp_sdk_bindgen/templates/bridge/oxfmt.json")
        .args(generated.keys().map(|name| out.join(name)))
        .status()
        .context("run browser bridge formatter")?;
    if !status.success() {
        bail!("browser bridge formatter failed: {status}");
    }
    Ok(())
}

fn contract_hash(groups: &MetadataGroupMap) -> String {
    let mut entries = groups
        .iter()
        .map(|(name, group)| (name, format!("{:?}", group.items)))
        .collect::<Vec<_>>();
    entries.sort();
    let mut hasher = Sha256::new();
    for (name, body) in entries {
        hasher.update(name.as_bytes());
        hasher.update([0]);
        hasher.update(body.as_bytes());
        hasher.update([0]);
    }
    format!("{:x}", hasher.finalize())
}

fn validate_bridge(items: &[Metadata]) -> Result<()> {
    for item in items {
        match item {
            Metadata::Method(method) if !method.is_async => {
                let immutable = method.inputs.is_empty()
                    && method.throws.is_none()
                    && method.return_type.is_some()
                    && matches!(
                        (method.self_name.as_str(), method.name.as_str()),
                        ("Client", "conversations" | "inbox_id" | "installation_id")
                            | ("Group", "id")
                    );
                if !immutable {
                    bail!(
                        "{}.{}: synchronous worker method is not an immutable property",
                        method.self_name,
                        method.name
                    );
                }
            }
            Metadata::TraitMethod(method) if !method.is_async => {
                if method.trait_name != "LogSink" || method.name != "log" {
                    bail!(
                        "{}.{}: synchronous foreign trait method",
                        method.trait_name,
                        method.name
                    );
                }
            }
            Metadata::Constructor(method) if !method.is_async => {
                bail!(
                    "{}.{}: synchronous constructor",
                    method.self_name,
                    method.name
                );
            }
            Metadata::Func(function) if !function.is_async => {
                bail!("{}: synchronous top-level function", function.name);
            }
            Metadata::ObjectTraitImpl(implementation) => {
                bail!(
                    "{:?}: exported Rust trait on bridged type",
                    implementation.ty
                );
            }
            Metadata::Object(object)
                if matches!(object.imp, ObjectImpl::Trait(TraitKind::RustOnly)) =>
            {
                bail!(
                    "{}: Rust-only trait cannot cross browser bridge",
                    object.name
                );
            }
            _ => {}
        }
        for ty in item_types(item) {
            validate_type(ty)?;
        }
    }
    Ok(())
}

fn item_types(item: &Metadata) -> Vec<&Type> {
    match item {
        Metadata::Func(value) => value
            .inputs
            .iter()
            .map(|p| &p.ty)
            .chain(value.return_type.iter())
            .chain(value.throws.iter())
            .collect(),
        Metadata::Method(value) => value
            .inputs
            .iter()
            .map(|p| &p.ty)
            .chain(value.return_type.iter())
            .chain(value.throws.iter())
            .collect(),
        Metadata::TraitMethod(value) => value
            .inputs
            .iter()
            .map(|p| &p.ty)
            .chain(value.return_type.iter())
            .chain(value.throws.iter())
            .collect(),
        Metadata::Constructor(value) => value
            .inputs
            .iter()
            .map(|p| &p.ty)
            .chain(value.throws.iter())
            .collect(),
        Metadata::Record(value) => value.fields.iter().map(|f| &f.ty).collect(),
        Metadata::Enum(value) => value
            .variants
            .iter()
            .flat_map(|v| v.fields.iter().map(|f| &f.ty))
            .collect(),
        Metadata::CustomType(value) => vec![&value.builtin],
        _ => Vec::new(),
    }
}

// Keep this match exhaustive. A new UniFFI type must stop generation until it is reviewed.
fn validate_type(ty: &Type) -> Result<()> {
    match ty {
        Type::UInt8
        | Type::Int8
        | Type::UInt16
        | Type::Int16
        | Type::UInt32
        | Type::Int32
        | Type::UInt64
        | Type::Int64
        | Type::Float32
        | Type::Float64
        | Type::Boolean
        | Type::String
        | Type::Bytes
        | Type::Timestamp
        | Type::Duration
        | Type::Object { .. }
        | Type::Record { .. }
        | Type::Enum { .. }
        | Type::CallbackInterface { .. } => Ok(()),
        Type::Box { inner_type }
        | Type::Optional { inner_type }
        | Type::Sequence { inner_type }
        | Type::Set { inner_type } => validate_type(inner_type),
        Type::Map {
            key_type,
            value_type,
        } => {
            validate_type(key_type)?;
            validate_type(value_type)
        }
        Type::Custom { builtin, .. } => validate_type(builtin),
    }
}

fn ts_name(source: &str, names: &BTreeMap<String, String>) -> String {
    let camel = source.to_lower_camel_case();
    names.get(&camel).cloned().unwrap_or(camel)
}

fn operations(items: &[Metadata], names: &BTreeMap<String, String>) -> Vec<Operation> {
    let mut output = Vec::new();
    for item in items {
        match item {
            Metadata::Method(value) => output.push(Operation {
                owner: Some(value.self_name.clone()),
                name: ts_name(&value.name, names),
                key: format!("{}.{}", value.self_name, ts_name(&value.name, names)),
                inputs: value
                    .inputs
                    .iter()
                    .map(|p| (ts_name(&p.name, names), p.ty.clone()))
                    .collect(),
                output: value.return_type.clone(),
                constructor: false,
                immutable: !value.is_async,
            }),
            Metadata::Constructor(value) => output.push(Operation {
                owner: Some(value.self_name.clone()),
                name: ts_name(&value.name, names),
                key: format!("{}.{}", value.self_name, ts_name(&value.name, names)),
                inputs: value
                    .inputs
                    .iter()
                    .map(|p| (ts_name(&p.name, names), p.ty.clone()))
                    .collect(),
                output: Some(Type::Object {
                    module_path: value.module_path.clone(),
                    name: value.self_name.clone(),
                    imp: ObjectImpl::Struct,
                }),
                constructor: true,
                immutable: false,
            }),
            Metadata::Func(value) => output.push(Operation {
                owner: None,
                name: ts_name(&value.name, names),
                key: ts_name(&value.name, names),
                inputs: value
                    .inputs
                    .iter()
                    .map(|p| (ts_name(&p.name, names), p.ty.clone()))
                    .collect(),
                output: value.return_type.clone(),
                constructor: false,
                immutable: false,
            }),
            _ => {}
        }
    }
    output.sort_by(|a, b| a.key.cmp(&b.key));
    output
}

fn wire_type(ty: &Type) -> String {
    match ty {
        Type::UInt8
        | Type::Int8
        | Type::UInt16
        | Type::Int16
        | Type::UInt32
        | Type::Int32
        | Type::Float32
        | Type::Float64 => "number".into(),
        Type::UInt64 | Type::Int64 | Type::Timestamp | Type::Duration => "bigint".into(),
        Type::Boolean => "boolean".into(),
        Type::String => "string".into(),
        Type::Bytes => "Uint8Array".into(),
        Type::Object { imp, .. } => {
            if imp.has_callback_interface() {
                "HandleWire | CallbackWire".into()
            } else {
                "HandleWire".into()
            }
        }
        Type::CallbackInterface { .. } => "CallbackWire".into(),
        Type::Record { name, .. } | Type::Enum { name, .. } => format!("Wire{name}"),
        Type::Box { inner_type } => wire_type(inner_type),
        Type::Optional { inner_type } => format!("{} | undefined", wire_type(inner_type)),
        Type::Sequence { inner_type } => format!("Array<{}>", wire_type(inner_type)),
        Type::Set { inner_type } => format!("Set<{}>", wire_type(inner_type)),
        Type::Map {
            key_type,
            value_type,
        } => format!("Map<{}, {}>", wire_type(key_type), wire_type(value_type)),
        Type::Custom { builtin, .. } => wire_type(builtin),
    }
}

fn ts_type(ty: &Type) -> String {
    match ty {
        Type::UInt8
        | Type::Int8
        | Type::UInt16
        | Type::Int16
        | Type::UInt32
        | Type::Int32
        | Type::Float32
        | Type::Float64 => "number".into(),
        Type::UInt64 | Type::Int64 => "bigint".into(),
        Type::Boolean => "boolean".into(),
        Type::String => "string".into(),
        Type::Bytes => "Uint8Array".into(),
        Type::Timestamp => "Date".into(),
        Type::Duration => "number".into(),
        Type::Object { name, imp, .. } => {
            if imp.has_struct() {
                format!("B.{name}Like")
            } else {
                format!("B.{name}")
            }
        }
        Type::Record { name, .. } | Type::Enum { name, .. } | Type::Custom { name, .. } => {
            format!("B.{name}")
        }
        Type::CallbackInterface { name, .. } => format!("B.{name}"),
        Type::Box { inner_type } => ts_type(inner_type),
        Type::Optional { inner_type } => format!("{} | undefined", ts_type(inner_type)),
        Type::Sequence { inner_type } => format!("Array<{}>", ts_type(inner_type)),
        Type::Set { inner_type } => format!("Set<{}>", ts_type(inner_type)),
        Type::Map {
            key_type,
            value_type,
        } => format!("Map<{}, {}>", ts_type(key_type), ts_type(value_type)),
    }
}

fn shape(ty: &Type) -> String {
    match ty {
        Type::UInt8
        | Type::Int8
        | Type::UInt16
        | Type::Int16
        | Type::UInt32
        | Type::Int32
        | Type::UInt64
        | Type::Int64
        | Type::Float32
        | Type::Float64
        | Type::Boolean
        | Type::String
        | Type::Bytes
        | Type::Timestamp
        | Type::Duration => format!("{{ kind: \"value\", type: \"{ty:?}\" }}"),
        Type::Object { name, imp, .. } => {
            if imp.has_callback_interface() {
                format!("{{ kind: \"foreign\", name: \"{name}\" }}")
            } else {
                format!("{{ kind: \"object\", name: \"{name}\" }}")
            }
        }
        Type::CallbackInterface { name, .. } => {
            format!("{{ kind: \"callback\", name: \"{name}\" }}")
        }
        Type::Record { name, .. } => format!("{{ kind: \"record\", name: \"{name}\" }}"),
        Type::Enum { name, .. } => format!("{{ kind: \"enum\", name: \"{name}\" }}"),
        Type::Box { inner_type }
        | Type::Custom {
            builtin: inner_type,
            ..
        } => shape(inner_type),
        Type::Optional { inner_type } => {
            format!("{{ kind: \"optional\", inner: {} }}", shape(inner_type))
        }
        Type::Sequence { inner_type } => {
            format!("{{ kind: \"sequence\", inner: {} }}", shape(inner_type))
        }
        Type::Set { inner_type } => format!("{{ kind: \"set\", inner: {} }}", shape(inner_type)),
        Type::Map {
            key_type,
            value_type,
        } => format!(
            "{{ kind: \"map\", key: {}, value: {} }}",
            shape(key_type),
            shape(value_type)
        ),
    }
}

fn render(
    items: &[Metadata],
    operations: &[Operation],
    hash: &str,
    names: &BTreeMap<String, String>,
) -> Result<BTreeMap<&'static str, String>> {
    let mut result = BTreeMap::new();
    let mut contract = format!(
        "export const PROTOCOL_VERSION = 1;\nexport const CONTRACT_HASH = \"{hash}\";\nexport const METHOD_KEYS = [\n"
    );
    for op in operations {
        writeln!(contract, "  \"{}\",", op.key)?;
    }
    contract.push_str("];\n");
    result.insert("contract.gen.ts", contract);

    let mut wire = String::from(
        "import type { HandleWire, CallbackWire, ErrorWire } from \"./runtime/bridge/wire.js\";\nimport type { Layouts, Shape } from \"./runtime/bridge/codec.js\";\n",
    );
    for item in items {
        match item {
            Metadata::Record(record) => {
                writeln!(wire, "export interface Wire{} {{", record.name)?;
                for field in &record.fields {
                    writeln!(
                        wire,
                        "  {}: {};",
                        ts_name(&field.name, names),
                        wire_type(&field.ty)
                    )?;
                }
                wire.push_str("}\n");
            }
            Metadata::Enum(value) => {
                write!(wire, "export type Wire{} = ", value.name)?;
                if value.shape.is_error() {
                    wire.push_str("ErrorWire");
                } else {
                    for (index, variant) in value.variants.iter().enumerate() {
                        if index > 0 {
                            wire.push_str(" | ");
                        }
                        write!(wire, "{{ tag: \"{}\"", variant.name)?;
                        if !variant.fields.is_empty() {
                            if variant.fields[0].name.is_empty() {
                                let fields = variant
                                    .fields
                                    .iter()
                                    .map(|f| wire_type(&f.ty))
                                    .collect::<Vec<_>>()
                                    .join(", ");
                                write!(wire, "; inner: [{fields}]")?;
                            } else {
                                wire.push_str("; inner: { ");
                                for field in &variant.fields {
                                    write!(
                                        wire,
                                        "{}: {}; ",
                                        ts_name(&field.name, names),
                                        wire_type(&field.ty)
                                    )?;
                                }
                                wire.push('}');
                            }
                        }
                        wire.push_str(" }");
                    }
                }
                wire.push_str(";\n");
            }
            _ => {}
        }
    }
    wire.push_str("export interface Calls {\n");
    for op in operations {
        let args = op
            .inputs
            .iter()
            .map(|(_, ty)| wire_type(ty))
            .collect::<Vec<_>>()
            .join(", ");
        let output = op
            .output
            .as_ref()
            .map(wire_type)
            .unwrap_or_else(|| "void".into());
        writeln!(
            wire,
            "  \"{}\": {{ args: [{args}]; result: {output} }};",
            op.key
        )?;
    }
    wire.push_str("}\n");
    wire.push_str("export const LAYOUTS = { records: {\n");
    for item in items {
        if let Metadata::Record(record) = item {
            writeln!(wire, "  {}: {{ fields: {{", record.name)?;
            for field in &record.fields {
                writeln!(
                    wire,
                    "    {}: {},",
                    ts_name(&field.name, names),
                    shape(&field.ty)
                )?;
            }
            wire.push_str("  } },\n");
        }
    }
    wire.push_str("}, enums: {\n");
    for item in items {
        if let Metadata::Enum(value) = item {
            writeln!(
                wire,
                "  {}: {{ error: {}, variants: {{",
                value.name,
                value.shape.is_error()
            )?;
            for variant in &value.variants {
                if variant.fields.first().is_some_and(|f| f.name.is_empty()) {
                    writeln!(wire, "    {}: [", variant.name)?;
                    for field in &variant.fields {
                        writeln!(wire, "      {},", shape(&field.ty))?;
                    }
                    wire.push_str("    ],\n");
                } else {
                    writeln!(wire, "    {}: {{", variant.name)?;
                    for field in &variant.fields {
                        writeln!(
                            wire,
                            "      {}: {},",
                            ts_name(&field.name, names),
                            shape(&field.ty)
                        )?;
                    }
                    wire.push_str("    },\n");
                }
            }
            wire.push_str("  } },\n");
        }
    }
    wire.push_str("} } satisfies Layouts;\n");
    wire.push_str("export const BRIDGED_OBJECTS = [\n");
    for item in items {
        if let Metadata::Object(object) = item {
            if object.imp.has_struct() {
                writeln!(wire, "  \"{}\",", object.name)?;
            }
        }
    }
    wire.push_str("];\nexport const FOREIGN_OBJECTS = [\n");
    for item in items {
        if let Metadata::Object(object) = item {
            if object.imp.has_callback_interface() {
                writeln!(wire, "  \"{}\",", object.name)?;
            }
        }
    }
    wire.push_str("];\n");
    wire.push_str("export interface ForeignMethod { inputs: Shape[]; output: Shape }\n");
    wire.push_str("export const FOREIGN_METHODS = {\n");
    let mut foreign_methods = BTreeMap::<String, Vec<_>>::new();
    for item in items {
        if let Metadata::TraitMethod(method) = item {
            foreign_methods
                .entry(method.trait_name.clone())
                .or_default()
                .push(method);
        }
    }
    for (trait_name, methods) in foreign_methods {
        writeln!(wire, "  {trait_name}: {{")?;
        for method in methods {
            let inputs = method
                .inputs
                .iter()
                .map(|input| shape(&input.ty))
                .collect::<Vec<_>>()
                .join(", ");
            let output = method
                .return_type
                .as_ref()
                .map(shape)
                .unwrap_or_else(|| "{ kind: \"value\" }".into());
            writeln!(
                wire,
                "    {}: {{ inputs: [{inputs}], output: {output} }},",
                ts_name(&method.name, names)
            )?;
        }
        wire.push_str("  },\n");
    }
    wire.push_str("} satisfies Record<string, Record<string, ForeignMethod>>;\n");
    result.insert("wire.gen.ts", wire);

    let mut proxy = String::from(
        "import * as B from \"./xmtp_sdk.js\";\nimport type { MainSession } from \"./runtime/bridge/main/session.js\";\nimport type { HandleWire } from \"./runtime/bridge/wire.js\";\nimport { RemoteObject } from \"./runtime/bridge/main/remote-object.js\";\nimport { enumFactory } from \"./runtime/bridge/codec.js\";\nimport { mainEncoder, mainDecoder } from \"./codec.main.gen.js\";\n",
    );
    for item in items {
        if let Metadata::Object(object) = item {
            if object.imp.has_struct() {
                writeln!(
                    proxy,
                    "export class {} extends RemoteObject implements B.{}Like {{",
                    object.name, object.name
                )?;
                for op in operations
                    .iter()
                    .filter(|op| op.owner.as_deref() == Some(&object.name) && op.constructor)
                {
                    let params = op
                        .inputs
                        .iter()
                        .map(|(name, ty)| format!("{name}: {}", ts_type(ty)))
                        .collect::<Vec<_>>()
                        .join(", ");
                    let args = op
                        .inputs
                        .iter()
                        .map(|(name, ty)| format!("encoder.convert({}, {name})", shape(ty)))
                        .collect::<Vec<_>>()
                        .join(", ");
                    let comma = if params.is_empty() { "" } else { ", " };
                    writeln!(
                        proxy,
                        "  static async {}(session: MainSession{comma}{params}, asyncOpts_?: {{ signal: AbortSignal }}): Promise<{}> {{",
                        op.name, object.name
                    )?;
                    if !op.inputs.is_empty() {
                        proxy.push_str("    const encoder = mainEncoder(session);\n");
                    }
                    writeln!(
                        proxy,
                        "    const handle = await session.call<HandleWire>(\"{}\", [{args}], undefined, asyncOpts_?.signal);",
                        op.key
                    )?;
                    writeln!(proxy, "    return new {}(session, handle);", object.name)?;
                    proxy.push_str("  }\n");
                }
                for op in operations
                    .iter()
                    .filter(|op| op.owner.as_deref() == Some(&object.name) && !op.constructor)
                {
                    let params = op
                        .inputs
                        .iter()
                        .map(|(name, ty)| format!("{name}: {}", ts_type(ty)))
                        .collect::<Vec<_>>()
                        .join(", ");
                    let args = op
                        .inputs
                        .iter()
                        .map(|(name, ty)| format!("encoder.convert({}, {name})", shape(ty)))
                        .collect::<Vec<_>>()
                        .join(", ");
                    let output = op
                        .output
                        .as_ref()
                        .map(ts_type)
                        .unwrap_or_else(|| "void".into());
                    let result_shape = op
                        .output
                        .as_ref()
                        .map(shape)
                        .unwrap_or_else(|| "{ kind: \"value\" }".into());
                    if op.immutable {
                        writeln!(
                            proxy,
                            "  {}(): {output} {{ return mainDecoder(this.session, enumFactory(B), (handle) => proxyFor(this.session, handle)).decode<{output}>({result_shape}, this.snapshot<unknown>(\"{}\")); }}",
                            op.name, op.name
                        )?;
                    } else if object.name == "Client" && op.name == "end" {
                        writeln!(
                            proxy,
                            "  async end(asyncOpts_?: {{ signal: AbortSignal }}): Promise<void> {{ try {{ await this.call<void>(\"Client.end\", [], asyncOpts_?.signal); }} finally {{ this.endOwner(); }} }}"
                        )?;
                    } else {
                        let comma = if params.is_empty() { "" } else { ", " };
                        writeln!(
                            proxy,
                            "  async {}({params}{comma}asyncOpts_?: {{ signal: AbortSignal }}): Promise<{output}> {{",
                            op.name
                        )?;
                        if !op.inputs.is_empty() {
                            proxy.push_str("    const encoder = mainEncoder(this.session);\n");
                        }
                        writeln!(
                            proxy,
                            "    const raw = await this.call<unknown>(\"{}\", [{args}], asyncOpts_?.signal);",
                            op.key
                        )?;
                        writeln!(
                            proxy,
                            "    return mainDecoder(this.session, enumFactory(B), (handle) => proxyFor(this.session, handle)).decode<{output}>({result_shape}, raw);"
                        )?;
                        proxy.push_str("  }\n");
                    }
                }
                proxy.push_str("}\n");
            }
        }
    }
    proxy.push_str("export function proxyFor(session: MainSession, handle: HandleWire): RemoteObject {\n  switch (handle.type) {\n");
    for item in items {
        if let Metadata::Object(object) = item {
            if object.imp.has_struct() {
                writeln!(
                    proxy,
                    "    case \"{}\": return new {}(session, handle);",
                    object.name, object.name
                )?;
            }
        }
    }
    proxy.push_str(
        "    default: throw new TypeError(`unknown object type ${handle.type}`);\n  }\n}\n",
    );
    result.insert("proxy.gen.ts", proxy);

    let mut dispatch = String::from(
        "import * as B from \"./xmtp_sdk.js\";\nimport type { Calls } from \"./wire.gen.js\";\nimport type { Shape } from \"./runtime/bridge/codec.js\";\nimport { enumFactory } from \"./runtime/bridge/codec.js\";\nimport type { WorkerContext } from \"./runtime/bridge/worker/host.js\";\nimport { poolName } from \"./runtime/bridge/worker/host.js\";\nimport { workerDecoder, workerEncoder } from \"./codec.worker.gen.js\";\n\ninterface MethodEntry { owner: string | null; name: string; inputs: Shape[]; output: Shape; constructor: boolean; immutable: boolean }\nexport const METHOD_TABLE = {\n",
    );
    for op in operations {
        let inputs = op
            .inputs
            .iter()
            .map(|(_, ty)| shape(ty))
            .collect::<Vec<_>>()
            .join(", ");
        let output = op
            .output
            .as_ref()
            .map(shape)
            .unwrap_or_else(|| "{ kind: \"value\" }".into());
        writeln!(
            dispatch,
            "  \"{}\": {{ owner: {}, name: \"{}\", inputs: [{inputs}], output: {output}, constructor: {}, immutable: {} }},",
            op.key,
            op.owner
                .as_ref()
                .map(|v| format!("\"{v}\""))
                .unwrap_or_else(|| "null".into()),
            op.name,
            op.constructor,
            op.immutable
        )?;
    }
    dispatch.push_str("} satisfies Record<keyof Calls, MethodEntry>;\n\n");
    dispatch.push_str("const methods: Partial<Record<string, MethodEntry>> = METHOD_TABLE;\n");
    dispatch.push_str(
        "const immutable: Partial<Record<string, Array<{ name: string; shape: Shape }>>> = {\n",
    );
    for item in items {
        if let Metadata::Object(object) = item {
            if object.imp.has_struct() {
                writeln!(dispatch, "  {}: [", object.name)?;
                for op in operations
                    .iter()
                    .filter(|op| op.owner.as_deref() == Some(&object.name) && op.immutable)
                {
                    if let Some(ty) = &op.output {
                        writeln!(
                            dispatch,
                            "    {{ name: \"{}\", shape: {} }},",
                            op.name,
                            shape(ty)
                        )?;
                    }
                }
                dispatch.push_str("  ],\n");
            }
        }
    }
    dispatch.push_str("};\n\n");
    dispatch.push_str("function snapshot(name: string, value: object, owner: number, context: WorkerContext): Record<string, unknown> {\n  const output: Record<string, unknown> = {};\n  for (const field of immutable[name] ?? []) {\n    const method: unknown = Reflect.get(value, field.name);\n    if (typeof method !== \"function\") throw new TypeError(`missing immutable method ${field.name}`);\n    const result: unknown = Reflect.apply(method, value, []);\n    output[field.name] = workerEncoder(context.registry, owner, (type, nested, nestedOwner) => snapshot(type, nested, nestedOwner, context)).convert(field.shape, result);\n  }\n  return output;\n}\n\n");
    dispatch.push_str("export async function dispatchGenerated(key: string, args: unknown[], context: WorkerContext): Promise<unknown> {\n  const operation = methods[key];\n  if (!operation) throw new TypeError(`unknown bridge method ${key}`);\n  const receiver: unknown = operation.constructor && operation.owner ? Reflect.get(B, operation.owner) : operation.owner ? context.target : B;\n  if (receiver === null || (typeof receiver !== \"object\" && typeof receiver !== \"function\")) throw new TypeError(`missing receiver for ${key}`);\n  const method: unknown = Reflect.get(receiver, operation.name);\n  if (typeof method !== \"function\") throw new TypeError(`missing binding method ${key}`);\n  const decoder = workerDecoder(context.registry, context.callbacks, enumFactory(B));\n  const decoded = operation.inputs.map((shape, index) => decoder.convert(shape, args[index]));\n  const pool = (key === \"Client.create\" || key === \"Client.build\") ? poolName(decoded[1]) : key === \"Storage.admin\" ? poolName(decoded[0]) : undefined;\n  let acquired = false;\n  if (pool) {\n    if (!context.locks) throw new TypeError(\"storage lock provider missing\");\n    acquired = await context.locks.open(pool);\n  }\n  try {\n    const callArgs = operation.immutable ? decoded : [...decoded, { signal: context.signal }];\n    const result: unknown = await Reflect.apply(method, receiver, callArgs);\n    const encoded = workerEncoder(context.registry, context.targetHandle?.owner, (type, value, owner) => snapshot(type, value, owner, context)).convert(operation.output, result);\n    if (pool && encoded !== null && typeof encoded === \"object\" && \"owner\" in encoded && typeof encoded.owner === \"number\") context.locks?.attachOwner(encoded.owner, pool);\n    return encoded;\n  } catch (error) {\n    if (pool && acquired) context.locks?.close(pool);\n    throw error;\n  }\n}\n");
    result.insert("dispatch.gen.ts", dispatch);

    for name in [
        "codec.main.gen.ts",
        "codec.worker.gen.ts",
        "stubs.gen.ts",
        "reverse.gen.ts",
        "conformance.gen.test.ts",
    ] {
        let template = match name {
            "codec.main.gen.ts" => include_str!("../../templates/bridge/codec.main.gen.ts"),
            "codec.worker.gen.ts" => include_str!("../../templates/bridge/codec.worker.gen.ts"),
            "stubs.gen.ts" => include_str!("../../templates/bridge/stubs.gen.ts"),
            "reverse.gen.ts" => include_str!("../../templates/bridge/reverse.gen.ts"),
            "conformance.gen.test.ts" => {
                include_str!("../../templates/bridge/conformance.gen.test.ts")
            }
            _ => unreachable!(),
        };
        result.insert(name, template.to_owned());
    }
    // The stock TypeScript backend applies type rename entries from uniffi.toml.
    // Apply the same entries to every generated bridge file, including shape keys.
    for body in result.values_mut() {
        for (source, target) in names {
            if source.chars().next().is_some_and(char::is_uppercase) {
                *body = body.replace(source, target);
            }
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use uniffi_meta::{
        ConstructorMetadata, FnMetadata, MethodMetadata, ObjectMetadata, ObjectTraitImplMetadata,
        TraitMethodMetadata,
    };

    #[xmtp_common::test(unwrap_try = true)]
    fn rejects_sync_constructor() {
        let item = Metadata::Constructor(ConstructorMetadata {
            module_path: "test".into(),
            self_name: "Client".into(),
            name: "new".into(),
            orig_name: None,
            is_async: false,
            inputs: vec![],
            throws: None,
            checksum: None,
            docstring: None,
        });
        assert!(
            validate_bridge(&[item])
                .unwrap_err()
                .to_string()
                .contains("Client.new")
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn rejects_sync_top_level_function() {
        let item = Metadata::Func(FnMetadata {
            module_path: "test".into(),
            name: "lookup".into(),
            orig_name: None,
            is_async: false,
            inputs: vec![],
            return_type: None,
            throws: None,
            checksum: None,
            docstring: None,
        });
        assert!(
            validate_bridge(&[item])
                .unwrap_err()
                .to_string()
                .contains("lookup")
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn rejects_sync_worker_method() {
        let item = Metadata::Method(MethodMetadata {
            module_path: "test".into(),
            self_name: "Group".into(),
            name: "send".into(),
            orig_name: None,
            is_async: false,
            inputs: vec![],
            return_type: None,
            throws: None,
            takes_self_by_arc: true,
            checksum: None,
            docstring: None,
        });
        assert!(
            validate_bridge(&[item])
                .unwrap_err()
                .to_string()
                .contains("Group.send")
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn rejects_unlisted_sync_getter() {
        let item = Metadata::Method(MethodMetadata {
            module_path: "test".into(),
            self_name: "Group".into(),
            name: "random_value".into(),
            orig_name: None,
            is_async: false,
            inputs: vec![],
            return_type: Some(Type::UInt64),
            throws: None,
            takes_self_by_arc: true,
            checksum: None,
            docstring: None,
        });
        assert!(
            validate_bridge(&[item])
                .unwrap_err()
                .to_string()
                .contains("Group.random_value")
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn rejects_sync_foreign_method() {
        let item = Metadata::TraitMethod(TraitMethodMetadata {
            module_path: "test".into(),
            trait_name: "Signer".into(),
            index: 0,
            name: "sign".into(),
            orig_name: None,
            is_async: false,
            inputs: vec![],
            return_type: None,
            throws: None,
            takes_self_by_arc: true,
            checksum: None,
            docstring: None,
        });
        assert!(
            validate_bridge(&[item])
                .unwrap_err()
                .to_string()
                .contains("Signer.sign")
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn rejects_exported_rust_trait() {
        let item = Metadata::ObjectTraitImpl(ObjectTraitImplMetadata {
            ty: Type::Object {
                module_path: "test".into(),
                name: "Client".into(),
                imp: ObjectImpl::Struct,
            },
            trait_ty: Type::Object {
                module_path: "test".into(),
                name: "Trait".into(),
                imp: ObjectImpl::Trait(TraitKind::RustOnly),
            },
        });
        assert!(
            validate_bridge(&[item])
                .unwrap_err()
                .to_string()
                .contains("Client")
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn new_method_changes_proxy_and_dispatch() {
        let object = Metadata::Object(ObjectMetadata {
            module_path: "test".into(),
            name: "Group".into(),
            orig_name: None,
            remote: false,
            imp: ObjectImpl::Struct,
            docstring: None,
        });
        let method = Metadata::Method(MethodMetadata {
            module_path: "test".into(),
            self_name: "Group".into(),
            name: "unread_total".into(),
            orig_name: None,
            is_async: true,
            inputs: vec![],
            return_type: Some(Type::UInt64),
            throws: None,
            takes_self_by_arc: true,
            checksum: None,
            docstring: None,
        });
        let items = [object, method];
        let names = BTreeMap::new();
        let operations = operations(&items, &names);
        let files = render(&items, &operations, "test", &names)?;
        assert!(files["proxy.gen.ts"].contains("unreadTotal"));
        assert!(files["dispatch.gen.ts"].contains("Group.unreadTotal"));
    }
}
