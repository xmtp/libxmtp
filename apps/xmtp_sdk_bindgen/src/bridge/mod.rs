use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
    fs,
};

use anyhow::{Context, Result, bail};
use camino::Utf8Path;
use heck::ToLowerCamelCase;
use sha2::{Digest, Sha256};
use ubrn_bindgen::wasm_metadata;
use uniffi_bindgen::{BindgenLoader, BindgenPaths, GlobalConfig};
use uniffi_meta::{Metadata, MetadataGroupMap, MethodMetadata, ObjectImpl, TraitKind, Type};

#[derive(Clone)]
struct Operation {
    owner: Option<String>,
    name: String,
    key: String,
    inputs: Vec<(String, Type)>,
    none_defaults: BTreeSet<String>,
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
    let operations = operations(&items);
    let hash = contract_hash(&metadata);
    fs::create_dir_all(out)?;
    let generated = render(&items, &operations, &hash)?;
    for (name, body) in &generated {
        fs::write(out.join(name), body)?;
    }
    let paths = generated
        .keys()
        .map(|name| out.join(name))
        .collect::<Vec<_>>();
    crate::format::typescript_files(paths.iter().map(|path| path.as_path()))
        .context("format the browser bridge")
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

/// A method the SDKs expose as a readonly property: no arguments, no error,
/// and a value.
fn getter_shaped(method: &MethodMetadata) -> bool {
    method.inputs.is_empty() && method.throws.is_none() && method.return_type.is_some()
}

/// The proxy reads a marked getter from a snapshot taken once, when the
/// handle is made. `#[sdk(immutable)]` in the façade writes the marker, and
/// the macro rejects an unmarked synchronous getter that reaches the browser,
/// so any other synchronous method here is a live read that must be async.
fn immutable_getter(method: &MethodMetadata) -> bool {
    getter_shaped(method)
        && crate::markers::has(method.docstring.as_deref(), crate::markers::IMMUTABLE)
}

fn validate_bridge(items: &[Metadata]) -> Result<()> {
    for item in items {
        if outside_bridge(item) {
            continue;
        }
        match item {
            Metadata::Method(method) if !method.is_async && !immutable_getter(method) => {
                if getter_shaped(method) {
                    bail!(
                        "{}.{}: synchronous getter is not marked immutable; mark a value that \
                         never changes with #[sdk(immutable)], or make a live read async",
                        method.self_name,
                        method.name
                    );
                }
                bail!(
                    "{}.{}: synchronous worker method takes arguments, returns a Result, or \
                     returns nothing; make it async",
                    method.self_name,
                    method.name
                );
            }
            Metadata::TraitMethod(method) if !method.is_async => {
                bail!(
                    "{}.{}: synchronous foreign trait method",
                    method.trait_name,
                    method.name
                );
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
            Metadata::UniffiTrait(implementation) => {
                bail!("{implementation:?}: UniFFI trait cannot cross browser bridge");
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
    validate_results(items)
}

/// Foreign traits whose objects the worker can hand to the main thread. Every
/// method is async, so a main-thread proxy forwards each call to the worker.
fn remote_foreign(items: &[Metadata]) -> BTreeSet<String> {
    items
        .iter()
        .filter_map(|item| match item {
            Metadata::Object(object) if object.imp.has_callback_interface() => items
                .iter()
                .all(|other| {
                    !matches!(other, Metadata::TraitMethod(method)
                        if method.trait_name == object.name && !method.is_async)
                })
                .then(|| object.name.clone()),
            _ => None,
        })
        .collect()
}

// A result the worker returns must decode on the main thread. A foreign
// object in a result becomes a proxy, which cannot serve a synchronous method.
fn validate_results(items: &[Metadata]) -> Result<()> {
    let remote = remote_foreign(items);
    for item in items {
        let (key, output) = match item {
            Metadata::Method(value) => (
                format!("{}.{}", value.self_name, value.name),
                &value.return_type,
            ),
            Metadata::Func(value) if !outside_bridge(item) => {
                (value.name.clone(), &value.return_type)
            }
            Metadata::TraitMethod(value) if remote.contains(&value.trait_name) => (
                format!("{}.{}", value.trait_name, value.name),
                &value.return_type,
            ),
            _ => continue,
        };
        if let Some(ty) = output
            && let Some(name) = unproxied_foreign(ty, items, &remote, &mut BTreeSet::new())
        {
            bail!("{key}: result can hold {name}, which has no main-thread proxy");
        }
    }
    Ok(())
}

fn unproxied_foreign(
    ty: &Type,
    items: &[Metadata],
    remote: &BTreeSet<String>,
    seen: &mut BTreeSet<String>,
) -> Option<String> {
    let mut check = |ty: &Type| unproxied_foreign(ty, items, remote, seen);
    match ty {
        Type::Object { name, imp, .. } if imp.has_callback_interface() => {
            (!remote.contains(name)).then(|| name.clone())
        }
        Type::CallbackInterface { name, .. } => Some(name.clone()),
        Type::Box { inner_type }
        | Type::Optional { inner_type }
        | Type::Sequence { inner_type }
        | Type::Set { inner_type } => check(inner_type),
        Type::Map {
            key_type,
            value_type,
        } => check(key_type).or_else(|| check(value_type)),
        Type::Custom { builtin, .. } => check(builtin),
        Type::Record { name, .. } | Type::Enum { name, .. } => {
            if !seen.insert(name.clone()) {
                return None;
            }
            let item = items.iter().find(|item| {
                matches!(item, Metadata::Record(value) if &value.name == name)
                    || matches!(item, Metadata::Enum(value) if &value.name == name)
            })?;
            item_types(item)
                .into_iter()
                .find_map(|ty| unproxied_foreign(ty, items, remote, seen))
        }
        _ => None,
    }
}

/// Read the worker-only marker for exported functions and methods.
pub(crate) fn worker_only(item: &Metadata) -> bool {
    let doc = match item {
        Metadata::Func(function) => function.docstring.as_deref(),
        Metadata::Method(method) => method.docstring.as_deref(),
        _ => None,
    };
    crate::markers::has(doc, crate::markers::WORKER)
}

/// Pure functions and worker-only calls do not cross the bridge.
fn outside_bridge(item: &Metadata) -> bool {
    worker_only(item)
        || matches!(item, Metadata::Func(function) if crate::markers::has(function.docstring.as_deref(), crate::markers::PURE))
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
        Type::Custom { name, builtin, .. } => {
            if !matches!(
                name.as_str(),
                "Message"
                    | "InboxId"
                    | "InstallationId"
                    | "ConversationId"
                    | "MessageId"
                    | "Timestamp"
                    | "ListenerId"
            ) {
                bail!("{name}: unsupported custom type");
            }
            validate_type(builtin)
        }
    }
}

fn ts_name(source: &str) -> String {
    source.to_lower_camel_case()
}

pub(super) fn none_defaults(inputs: &[uniffi_meta::FnParamMetadata]) -> BTreeSet<String> {
    inputs
        .iter()
        .rev()
        .take_while(|input| {
            matches!(
                input.default,
                Some(uniffi_meta::DefaultValueMetadata::Literal(
                    uniffi_meta::LiteralMetadata::None
                ))
            )
        })
        .map(|input| ts_name(&input.name))
        .collect()
}

fn operations(items: &[Metadata]) -> Vec<Operation> {
    let remote = remote_foreign(items);
    let mut output = Vec::new();
    for item in items {
        match item {
            Metadata::TraitMethod(value) if remote.contains(&value.trait_name) => {
                output.push(Operation {
                    owner: Some(value.trait_name.clone()),
                    name: ts_name(&value.name),
                    key: format!("{}.{}", value.trait_name, ts_name(&value.name)),
                    inputs: value
                        .inputs
                        .iter()
                        .map(|p| (ts_name(&p.name), p.ty.clone()))
                        .collect(),
                    none_defaults: none_defaults(&value.inputs),
                    output: value.return_type.clone(),
                    constructor: false,
                    immutable: false,
                })
            }
            Metadata::Method(value) if !outside_bridge(item) => output.push(Operation {
                owner: Some(value.self_name.clone()),
                name: ts_name(&value.name),
                key: format!("{}.{}", value.self_name, ts_name(&value.name)),
                inputs: value
                    .inputs
                    .iter()
                    .map(|p| (ts_name(&p.name), p.ty.clone()))
                    .collect(),
                none_defaults: none_defaults(&value.inputs),
                output: value.return_type.clone(),
                constructor: false,
                immutable: !value.is_async,
            }),
            Metadata::Constructor(value) => output.push(Operation {
                owner: Some(value.self_name.clone()),
                name: ts_name(&value.name),
                key: format!("{}.{}", value.self_name, ts_name(&value.name)),
                inputs: value
                    .inputs
                    .iter()
                    .map(|p| (ts_name(&p.name), p.ty.clone()))
                    .collect(),
                none_defaults: none_defaults(&value.inputs),
                output: Some(Type::Object {
                    module_path: value.module_path.clone(),
                    name: value.self_name.clone(),
                    imp: ObjectImpl::Struct,
                }),
                constructor: true,
                immutable: false,
            }),
            Metadata::Func(value) if !outside_bridge(item) => output.push(Operation {
                owner: None,
                name: ts_name(&value.name),
                key: ts_name(&value.name),
                inputs: value
                    .inputs
                    .iter()
                    .map(|p| (ts_name(&p.name), p.ty.clone()))
                    .collect(),
                none_defaults: none_defaults(&value.inputs),
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
        Type::Bytes => "ArrayBuffer".into(),
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
        Type::Bytes => "ArrayBuffer".into(),
        Type::Timestamp => "Date".into(),
        Type::Duration => "number".into(),
        Type::Object { name, imp, .. } => {
            if imp.has_struct() {
                format!("B.{name}Like")
            } else {
                format!("B.{name}")
            }
        }
        Type::Custom { name, .. }
            if matches!(
                name.as_str(),
                "InboxId" | "InstallationId" | "ConversationId" | "MessageId"
            ) =>
        {
            "string".into()
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
        Type::Box { inner_type } => shape(inner_type),
        Type::Custom { name, builtin, .. } => {
            if matches!(
                name.as_str(),
                "ListenerId" | "InboxId" | "InstallationId" | "ConversationId" | "MessageId"
            ) {
                return shape(builtin);
            }
            format!(
                "{{ kind: \"custom\", name: \"{name}\", inner: {} }}",
                shape(builtin)
            )
        }
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

fn decode_expr(ty: &Type, raw: &str, session: &str) -> String {
    match ty {
        Type::UInt8
        | Type::Int8
        | Type::UInt16
        | Type::Int16
        | Type::UInt32
        | Type::Int32
        | Type::Float32
        | Type::Float64
        | Type::Duration => format!("bridgeNumber({raw})"),
        Type::UInt64 | Type::Int64 => format!("bridgeBigInt({raw})"),
        Type::Boolean => format!("bridgeBoolean({raw})"),
        Type::String => format!("bridgeString({raw})"),
        Type::Bytes => format!("bridgeBytes({raw})"),
        Type::Timestamp => format!("bridgeDate({raw})"),
        Type::Object { name, .. } => format!("decodeObject{name}({session}, {raw})"),
        Type::CallbackInterface { name, .. } => format!("decodeObject{name}({session}, {raw})"),
        Type::Record { name, .. } => format!("decodeRecord{name}({session}, {raw})"),
        Type::Enum { name, .. } => format!("decodeEnum{name}({session}, {raw})"),
        Type::Box { inner_type } => decode_expr(inner_type, raw, session),
        Type::Custom { name, builtin, .. } => {
            let inner = decode_expr(builtin, raw, session);
            match name.as_str() {
                "Message" => format!("new HostMessage({inner}, {session})"),
                "Timestamp" => format!("new B.Timestamp({inner})"),
                "ListenerId" | "InboxId" | "InstallationId" | "ConversationId" | "MessageId" => {
                    inner
                }
                _ => format!("B.{name}.fromRust({inner})"),
            }
        }
        Type::Optional { inner_type } => format!(
            "({raw} === undefined || {raw} === null ? undefined : {})",
            decode_expr(inner_type, raw, session)
        ),
        Type::Sequence { inner_type } => format!(
            "bridgeArray({raw}).map((item) => {})",
            decode_expr(inner_type, "item", session)
        ),
        Type::Set { inner_type } => format!(
            "new Set(Array.from(bridgeSet({raw}), (item) => {}))",
            decode_expr(inner_type, "item", session)
        ),
        Type::Map {
            key_type,
            value_type,
        } => format!(
            "new Map(Array.from(bridgeMap({raw}), ([key, item]): [{}, {}] => [{}, {}]))",
            ts_type(key_type),
            ts_type(value_type),
            decode_expr(key_type, "key", session),
            decode_expr(value_type, "item", session)
        ),
    }
}

fn render_decoders(items: &[Metadata]) -> Result<String> {
    let remote = remote_foreign(items);
    let mut code = String::from(
        "function bridgeRecord(raw: unknown): Record<string, unknown> { if (raw === null || typeof raw !== \"object\" || Array.isArray(raw)) throw new TypeError(\"expected record\"); return Object.fromEntries(Object.entries(raw)); }\n\
function bridgeArray(raw: unknown): unknown[] { if (!Array.isArray(raw)) throw new TypeError(\"expected array\"); return raw; }\n\
function bridgeMap(raw: unknown): Map<unknown, unknown> { if (!(raw instanceof Map)) throw new TypeError(\"expected map\"); return raw; }\n\
function bridgeSet(raw: unknown): Set<unknown> { if (!(raw instanceof Set)) throw new TypeError(\"expected set\"); return raw; }\n\
function bridgeNumber(raw: unknown): number { if (typeof raw !== \"number\") throw new TypeError(\"expected number\"); return raw; }\n\
function bridgeBigInt(raw: unknown): bigint { if (typeof raw !== \"bigint\") throw new TypeError(\"expected bigint\"); return raw; }\n\
function bridgeString(raw: unknown): string { if (typeof raw !== \"string\") throw new TypeError(\"expected string\"); return raw; }\n\
function bridgeBoolean(raw: unknown): boolean { if (typeof raw !== \"boolean\") throw new TypeError(\"expected boolean\"); return raw; }\n\
function bridgeBytes(raw: unknown): ArrayBuffer { if (!(raw instanceof ArrayBuffer)) throw new TypeError(\"expected bytes\"); return raw; }\n\
function bridgeDate(raw: unknown): Date { if (!(raw instanceof Date)) throw new TypeError(\"expected date\"); return raw; }\n\
function bridgeHandle(raw: unknown, type: string): HandleWire { const value = bridgeRecord(raw); if (typeof value.h !== \"number\" || typeof value.owner !== \"number\" || typeof value.epoch !== \"number\" || value.type !== type) throw new TypeError(\"invalid handle\"); return { h: value.h, owner: value.owner, epoch: value.epoch, type, snap: value.snap }; }\n",
    );
    for item in items {
        match item {
            Metadata::Object(object)
                if object.imp.has_struct() || remote.contains(&object.name) =>
            {
                writeln!(
                    code,
                    "function decodeObject{}(session: MainSession, raw: unknown): {} {{ const value = proxyFor(session, bridgeHandle(raw, \"{}\")); if (!(value instanceof {})) throw new TypeError(\"invalid object\"); return value; }}",
                    object.name, object.name, object.name, object.name
                )?;
            }
            Metadata::Object(object) if object.imp.has_callback_interface() => {
                writeln!(
                    code,
                    "function decodeObject{}(_session: MainSession, _raw: unknown): B.{} {{ throw new TypeError(\"foreign object cannot be returned by worker\"); }}",
                    object.name, object.name
                )?;
            }
            Metadata::Record(record) => {
                if record.fields.is_empty() {
                    writeln!(
                        code,
                        "function decodeRecord{}(_session: MainSession, raw: unknown): B.{} {{ bridgeRecord(raw); return {{}}; }}",
                        record.name, record.name
                    )?;
                    continue;
                }
                writeln!(
                    code,
                    "function decodeRecord{}(session: MainSession, raw: unknown): B.{} {{ const fields = bridgeRecord(raw); return {{",
                    record.name, record.name
                )?;
                for field in &record.fields {
                    let name = ts_name(&field.name);
                    writeln!(
                        code,
                        "  {name}: {},",
                        decode_expr(&field.ty, &format!("fields.{name}"), "session")
                    )?;
                }
                code.push_str("}; }\n");
            }
            Metadata::Enum(value) => {
                writeln!(
                    code,
                    "function decodeEnum{}(session: MainSession, raw: unknown): B.{} {{",
                    value.name, value.name
                )?;
                let flat =
                    !value.shape.is_error() && value.variants.iter().all(|v| v.fields.is_empty());
                if flat {
                    code.push_str("switch (bridgeNumber(raw)) {\n");
                    for (index, variant) in value.variants.iter().enumerate() {
                        writeln!(
                            code,
                            "case {index}: return B.{}.{};",
                            value.name, variant.name
                        )?;
                    }
                } else {
                    code.push_str("const fields = bridgeRecord(raw); switch (fields.");
                    code.push_str(if value.shape.is_error() {
                        "variant"
                    } else {
                        "tag"
                    });
                    code.push_str(") {\n");
                    for variant in &value.variants {
                        let args = if value.shape.is_error() {
                            if variant.fields.first().is_some_and(|f| !f.name.is_empty()) {
                                let entries = variant
                                    .fields
                                    .iter()
                                    .map(|field| {
                                        let name = ts_name(&field.name);
                                        format!(
                                            "{name}: {}",
                                            decode_expr(
                                                &field.ty,
                                                &format!(
                                                    "bridgeRecord(bridgeArray(fields.details)[0]).{name}"
                                                ),
                                                "session"
                                            )
                                        )
                                    })
                                    .collect::<Vec<_>>()
                                    .join(", ");
                                format!("{{ {entries} }}")
                            } else {
                                variant
                                    .fields
                                    .iter()
                                    .enumerate()
                                    .map(|(index, field)| {
                                        decode_expr(
                                            &field.ty,
                                            &format!("bridgeArray(fields.details)[{index}]"),
                                            "session",
                                        )
                                    })
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            }
                        } else if variant.fields.first().is_some_and(|f| f.name.is_empty()) {
                            variant
                                .fields
                                .iter()
                                .enumerate()
                                .map(|(index, field)| {
                                    decode_expr(
                                        &field.ty,
                                        &format!("bridgeArray(fields.inner)[{index}]"),
                                        "session",
                                    )
                                })
                                .collect::<Vec<_>>()
                                .join(", ")
                        } else if variant.fields.is_empty() {
                            String::new()
                        } else {
                            let entries = variant
                                .fields
                                .iter()
                                .map(|field| {
                                    let name = ts_name(&field.name);
                                    format!(
                                        "{name}: {}",
                                        decode_expr(
                                            &field.ty,
                                            &format!("bridgeRecord(fields.inner).{name}"),
                                            "session"
                                        )
                                    )
                                })
                                .collect::<Vec<_>>()
                                .join(", ");
                            format!("{{ {entries} }}")
                        };
                        writeln!(
                            code,
                            "case \"{}\": return B.{}.{}.new({args});",
                            variant.name, value.name, variant.name
                        )?;
                    }
                }
                writeln!(
                    code,
                    "default: throw new TypeError(\"invalid {} variant\"); }} }}",
                    value.name
                )?;
            }
            _ => {}
        }
    }
    Ok(code
        .replace("function bridge", "export function bridge")
        .replace("function decode", "export function decode"))
}

fn render(
    items: &[Metadata],
    operations: &[Operation],
    hash: &str,
) -> Result<BTreeMap<&'static str, String>> {
    let mut result = BTreeMap::new();
    let mut contract = format!(
        "export const PROTOCOL_VERSION = 4;\nexport const CONTRACT_HASH = \"{hash}\";\nexport const METHOD_KEYS = [\n"
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
                if record.fields.is_empty() {
                    writeln!(
                        wire,
                        "export type Wire{} = Record<string, never>;",
                        record.name
                    )?;
                    continue;
                }
                writeln!(wire, "export interface Wire{} {{", record.name)?;
                for field in &record.fields {
                    writeln!(
                        wire,
                        "  {}: {};",
                        ts_name(&field.name),
                        wire_type(&field.ty)
                    )?;
                }
                wire.push_str("}\n");
            }
            Metadata::Enum(value) => {
                write!(wire, "export type Wire{} = ", value.name)?;
                if value.shape.is_error() {
                    wire.push_str("ErrorWire");
                } else if value
                    .variants
                    .iter()
                    .all(|variant| variant.fields.is_empty())
                {
                    wire.push_str("number");
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
                                        ts_name(&field.name),
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
                writeln!(wire, "    {}: {},", ts_name(&field.name), shape(&field.ty))?;
            }
            wire.push_str("  } },\n");
        }
    }
    wire.push_str("}, enums: {\n");
    for item in items {
        if let Metadata::Enum(value) = item {
            writeln!(
                wire,
                "  {}: {{ error: {}, flat: {}, variants: {{",
                value.name,
                value.shape.is_error(),
                !value.shape.is_error()
                    && value
                        .variants
                        .iter()
                        .all(|variant| variant.fields.is_empty())
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
                            ts_name(&field.name),
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
        if let Metadata::Object(object) = item
            && object.imp.has_struct()
        {
            writeln!(wire, "  \"{}\",", object.name)?;
        }
    }
    wire.push_str("];\nexport const FOREIGN_OBJECTS = [\n");
    for item in items {
        if let Metadata::Object(object) = item
            && object.imp.has_callback_interface()
        {
            writeln!(wire, "  \"{}\",", object.name)?;
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
                ts_name(&method.name)
            )?;
        }
        wire.push_str("  },\n");
    }
    wire.push_str("} satisfies Record<string, Record<string, ForeignMethod>>;\n");
    result.insert("wire.gen.ts", wire);

    let mut proxy = String::from(
        "import * as B from \"./xmtp_sdk.js\";\nimport { initPureWasm } from \"../typescript-pure/binding.js\";\nimport { Message as HostMessage, registerClient, resolveBrowserOptions, unregisterClient, type HostClientOptions } from \"./host-message.gen.js\";\nimport type { MainSession } from \"./runtime/bridge/main/session.js\";\nimport { decodeError, type ErrorWire, type HandleWire } from \"./runtime/bridge/wire.js\";\nimport { RemoteObject, endOwner } from \"./runtime/bridge/main/remote-object.js\";\nimport { mainEncoder } from \"./codec.main.gen.js\";\n",
    );
    let has_storage_admin = items
        .iter()
        .any(|item| matches!(item, Metadata::Object(object) if object.name == "StorageAdmin"));
    if has_storage_admin {
        proxy.push_str("import { openStorageAdmin, type StorageAdmin as PublicStorageAdmin } from \"./storage-admin.gen.js\";\n");
        result.insert(
            "storage-admin.gen.ts",
            include_str!("../../templates/bridge/storage-admin.gen.ts").into(),
        );
    }
    let remote = remote_foreign(items);
    for item in items {
        if let Metadata::Object(object) = item
            && (object.imp.has_struct() || remote.contains(&object.name))
        {
            // A foreign trait proxy implements the trait interface itself.
            let like = if object.imp.has_struct() { "Like" } else { "" };
            let refs = items.iter().collect::<Vec<_>>();
            let interface = crate::forwarding::bridged_interface(
                &refs,
                &object.name,
                &format!("B.{}{like}", object.name),
            );
            writeln!(
                proxy,
                "export class {} extends RemoteObject implements {interface} {{",
                object.name
            )?;
            if has_storage_admin && object.name == "Storage" {
                proxy.push_str("  static admin(): Promise<PublicStorageAdmin> { return openStorageAdmin(); }\n");
            }
            for op in operations
                .iter()
                .filter(|op| op.owner.as_deref() == Some(&object.name) && op.constructor)
            {
                let params = op
                    .inputs
                    .iter()
                    .map(|(name, ty)| {
                        let ty = if object.name == "Client" && name == "options" {
                            "HostClientOptions".into()
                        } else {
                            ts_type(ty)
                        };
                        format!("{name}: {ty}")
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                let args = op
                    .inputs
                    .iter()
                    .map(|(name, ty)| {
                        let value = if object.name == "Client" && name == "options" {
                            "resolvedBridgeOptions"
                        } else {
                            name
                        };
                        format!("encoder.convert({}, {value})", shape(ty))
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                let comma = if params.is_empty() { "" } else { ", " };
                writeln!(
                    proxy,
                    "  static async {}(session: MainSession{comma}{params}, asyncOpts_?: {{ signal: AbortSignal }}): Promise<{}> {{",
                    op.name, object.name
                )?;
                if !op.inputs.is_empty() {
                    if object.name == "Client" {
                        proxy.push_str("    await initPureWasm();\n    const { codecs = [], ...bridgeOptions } = options;\n    const resolvedBridgeOptions = resolveBrowserOptions(bridgeOptions);\n");
                    }
                    proxy.push_str("    const encoder = mainEncoder(session);\n");
                }
                writeln!(
                    proxy,
                    "    installErrorDecoder(session);\n    const handle = bridgeHandle(await session.call(\"{}\", () => [{args}], undefined, asyncOpts_?.signal), \"{}\");",
                    op.key, object.name
                )?;
                if object.name == "Client" {
                    proxy.push_str("    const client = decodeObjectClient(session, handle);\n    registerClient(session, client, codecs);\n    return client;\n");
                } else {
                    writeln!(
                        proxy,
                        "    return decodeObject{}(session, handle);",
                        object.name
                    )?;
                }
                proxy.push_str("  }\n");
            }
            for op in operations
                .iter()
                .filter(|op| op.owner.as_deref() == Some(&object.name) && !op.constructor)
            {
                let params = op
                    .inputs
                    .iter()
                    .map(|(name, ty)| {
                        let optional = if op.none_defaults.contains(name) {
                            "?"
                        } else {
                            ""
                        };
                        let ty = match ty {
                            Type::Optional { inner_type } if !optional.is_empty() => inner_type,
                            ty => ty,
                        };
                        format!("{name}{optional}: {}", ts_type(ty))
                    })
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
                if op.immutable {
                    let value = decode_expr(
                        op.output.as_ref().expect("immutable result"),
                        &format!("this.snapshot(\"{}\")", op.name),
                        "this.session",
                    );
                    writeln!(
                        proxy,
                        "  {}(): {output} {{ return this.held(() => {value}); }}",
                        op.name
                    )?;
                } else if object.name == "Client" && op.name == "end" {
                    writeln!(
                        proxy,
                        "  private closing?: Promise<void>;\n  async end(asyncOpts_?: {{ signal: AbortSignal }}): Promise<void> {{ if (!this.closing) {{ const key = this.clientKey(); const call = this.call(\"Client.end\", [], asyncOpts_?.signal); this.fence(); this.closing = call.then(() => {{ endOwner(this); unregisterClient(this.session, key); }}, (error: unknown) => {{ this.unfence(); this.closing = undefined; throw error; }}); }} return this.closing; }}"
                    )?;
                } else if object.name == "StorageAdmin" && op.name == "end" {
                    proxy.push_str("  private closing?: Promise<void>;\n  async end(asyncOpts_?: { signal: AbortSignal }): Promise<void> { if (!this.closing) { const call = this.call(\"StorageAdmin.end\", [], asyncOpts_?.signal); this.fence(); this.closing = call.then(() => { endOwner(this); }, (error: unknown) => { this.unfence(); this.closing = undefined; throw error; }); } return this.closing; }\n");
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
                    let binding = if op.output.is_some() {
                        "const raw = "
                    } else {
                        ""
                    };
                    writeln!(
                        proxy,
                        "    installErrorDecoder(this.session);\n    {binding}await this.call(\"{}\", () => [{args}], asyncOpts_?.signal);",
                        op.key
                    )?;
                    let value = op
                        .output
                        .as_ref()
                        .map(|ty| decode_expr(ty, "raw", "this.session"))
                        .unwrap_or_else(|| "undefined".into());
                    writeln!(proxy, "    return {value};")?;
                    proxy.push_str("  }\n");
                }
            }
            proxy.push_str("}\n");
        }
    }
    // Each exported function runs in the worker. The package Client wraps these
    // with the package session; apps do not pass a session.
    for op in operations.iter().filter(|op| op.owner.is_none()) {
        let parameters = op
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
        let result = op
            .output
            .as_ref()
            .map(ts_type)
            .unwrap_or_else(|| "void".into());
        let comma = if parameters.is_empty() { "" } else { ", " };
        writeln!(
            proxy,
            "export async function {}(session: MainSession{comma}{parameters}): Promise<{result}> {{\n{}  installErrorDecoder(session);\n  const raw = await session.call(\"{}\", () => [{args}]);",
            op.name,
            if op.inputs.is_empty() {
                ""
            } else {
                "  const encoder = mainEncoder(session);\n"
            },
            op.key
        )?;
        match &op.output {
            Some(ty) => writeln!(proxy, "  return {};", decode_expr(ty, "raw", "session"))?,
            None => proxy.push_str("  void raw;\n"),
        }
        proxy.push_str("}\n");
    }
    proxy.push_str("export function proxyFor(session: MainSession, handle: HandleWire): RemoteObject {\n  session.checkHandle(handle);\n  const existing = session.proxy(handle); if (existing) return existing;\n  switch (handle.type) {\n");
    for item in items {
        if let Metadata::Object(object) = item
            && (object.imp.has_struct() || remote.contains(&object.name))
        {
            writeln!(
                proxy,
                "    case \"{}\": return new {}(session, handle);",
                object.name, object.name
            )?;
        }
    }
    proxy.push_str(
        "    default: throw new TypeError(`unknown object type ${handle.type}`);\n  }\n}\n",
    );
    proxy.push_str(&render_decoders(items)?);
    if items.iter().any(|item| matches!(item, Metadata::Enum(value) if value.shape.is_error() && value.name == "XmtpError")) {
        proxy.push_str("function installErrorDecoder(session: MainSession): void { session.setErrorDecoder((wire: ErrorWire): Error => { try { return decodeEnumXmtpError(session, wire); } catch { return decodeError(wire); } }); }\n");
    } else {
        proxy.push_str("function installErrorDecoder(session: MainSession): void { session.setErrorDecoder(decodeError); }\n");
    }
    result.insert("proxy.gen.ts", proxy);
    result.insert(
        "host-message.gen.ts",
        include_str!("../../templates/bridge/host-message.gen.ts").into(),
    );

    let mut dispatch = String::from(
        "import * as B from \"./xmtp_sdk.js\";\nimport type { Calls } from \"./wire.gen.js\";\nimport type { Shape } from \"./runtime/bridge/codec.js\";\nimport { enumFactory } from \"./runtime/bridge/codec.js\";\nimport type { WorkerContext } from \"./runtime/bridge/worker/host.js\";\nimport { callWithPool, checkTarget, poolName } from \"./runtime/bridge/worker/host.js\";\nimport { workerDecoder, workerEncoder } from \"./codec.worker.gen.js\";\n\ninterface MethodEntry { owner: string | null; name: string; inputs: Shape[]; output: Shape; constructor: boolean; immutable: boolean }\nexport const METHOD_TABLE = {\n",
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
        if let Metadata::Object(object) = item
            && object.imp.has_struct()
        {
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
    dispatch.push_str("};\n\n");
    dispatch.push_str("function snapshot(name: string, value: object, owner: number, context: WorkerContext): Record<string, unknown> {\n  const output: Record<string, unknown> = {};\n  for (const field of immutable[name] ?? []) {\n    const method: unknown = Reflect.get(value, field.name);\n    if (typeof method !== \"function\") throw new TypeError(`missing immutable method ${field.name}`);\n    const result: unknown = Reflect.apply(method, value, []);\n    output[field.name] = workerEncoder(context.registry, owner, (type, nested, nestedOwner) => snapshot(type, nested, nestedOwner, context)).convert(field.shape, result);\n  }\n  return output;\n}\n\n");
    // Every persistent store opens one OPFS pool in this directory, so the
    // directory also names the storage lock of that pool.
    writeln!(
        dispatch,
        "const STORAGE_POOL = {:?};",
        xmtp_configuration::WASM_VFS_DIRECTORY
    )?;
    dispatch.push_str("export async function dispatchGenerated(key: string, args: unknown[], context: WorkerContext): Promise<unknown> {\n  const operation = methods[key];\n  if (!operation) throw new TypeError(`unknown bridge method ${key}`);\n  checkTarget(key, operation.owner !== null && !operation.constructor ? operation.owner : undefined, context);\n  const receiver: unknown = operation.constructor && operation.owner ? Reflect.get(B, operation.owner) : operation.owner ? context.target : B;\n  if (receiver === null || (typeof receiver !== \"object\" && typeof receiver !== \"function\")) throw new TypeError(`missing receiver for ${key}`);\n  const method: unknown = Reflect.get(receiver, operation.name);\n  if (typeof method !== \"function\") throw new TypeError(`missing binding method ${key}`);\n  const decoder = workerDecoder(context.registry, context.callbacks, enumFactory(B));\n  const decoded = operation.inputs.map((shape, index) => decoder.convert(shape, args[index]));\n  const createsClient = key === \"Client.create\" || key === \"Client.build\";\n  const createsAdmin = key === \"StorageAdmin.open\";\n  const pool = createsClient ? poolName(decoded[1], STORAGE_POOL) : createsAdmin ? STORAGE_POOL : context.targetHandle ? context.locks?.poolForOwner(context.targetHandle.owner) : undefined;\n  const callArgs = operation.immutable ? decoded : [...decoded, { signal: context.signal }];\n  const value = await callWithPool(context.locks, pool, createsClient || createsAdmin, () => Reflect.apply(method, receiver, callArgs), (result) => context.registry.scope(() => workerEncoder(context.registry, context.targetHandle?.owner, (type, value, owner) => snapshot(type, value, owner, context)).convert(operation.output, result)), B.storageRequiresWorkerRestart, context.started, (owner) => { context.createdOwner = owner; });\n  if (context.settled) context.settled();\n  return value;\n}\n");
    result.insert("dispatch.gen.ts", dispatch);
    result.insert(
        "worker-entry.gen.js",
        "import \"./worker.gen.js\";\n".into(),
    );

    for name in [
        "package-session.gen.ts",
        "worker.gen.ts",
        "codec.main.gen.ts",
        "codec.worker.gen.ts",
        "stubs.gen.ts",
        "reverse.gen.ts",
        "public-client.gen.ts",
        "conformance.gen.test.ts",
    ] {
        let template = match name {
            "package-session.gen.ts" => {
                include_str!("../../templates/bridge/package-session.gen.ts")
            }
            "worker.gen.ts" => include_str!("../../templates/bridge/worker.gen.ts"),
            "codec.main.gen.ts" => include_str!("../../templates/bridge/codec.main.gen.ts"),
            "codec.worker.gen.ts" => include_str!("../../templates/bridge/codec.worker.gen.ts"),
            "stubs.gen.ts" => include_str!("../../templates/bridge/stubs.gen.ts"),
            "reverse.gen.ts" => include_str!("../../templates/bridge/reverse.gen.ts"),
            "public-client.gen.ts" => include_str!("../../templates/bridge/public-client.gen.ts"),
            "conformance.gen.test.ts" => {
                include_str!("../../templates/bridge/conformance.gen.test.ts")
            }
            _ => unreachable!(),
        };
        result.insert(name, template.to_owned());
    }
    Ok(result)
}

#[cfg(test)]
mod conformance_methods;

#[cfg(test)]
mod tests {
    use super::*;
    use uniffi_meta::{
        ConstructorMetadata, EnumMetadata, EnumShape, FieldMetadata, FnMetadata, MethodMetadata,
        ObjectMetadata, ObjectTraitImplMetadata, TraitMethodMetadata, UniffiTraitMetadata,
        VariantMetadata,
    };

    #[xmtp_common::test(unwrap_try = true)]
    fn public_time_types_match_node_runtime() {
        let timestamp = Type::Custom {
            module_path: "test".into(),
            name: "Timestamp".into(),
            builtin: Box::new(Type::Int64),
        };
        assert_eq!(ts_type(&timestamp), "B.Timestamp");
        assert_eq!(ts_type(&Type::Timestamp), "Date");
        assert_eq!(ts_type(&Type::Duration), "number");
        assert_eq!(wire_type(&timestamp), "bigint");
        assert_eq!(wire_type(&Type::Duration), "bigint");
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn message_lift_uses_browser_host_class() {
        let message = Type::Custom {
            module_path: "test".into(),
            name: "Message".into(),
            builtin: Box::new(Type::Record {
                module_path: "test".into(),
                name: "MessageData".into(),
            }),
        };
        assert_eq!(
            decode_expr(&message, "raw", "session"),
            "new HostMessage(decodeRecordMessageData(session, raw), session)"
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn named_error_variant_uses_named_constructor() {
        let item = Metadata::Enum(EnumMetadata {
            module_path: "test".into(),
            name: "LogSinkError".into(),
            orig_name: None,
            shape: EnumShape::Error { flat: false },
            remote: false,
            variants: vec![VariantMetadata {
                name: "Failed".into(),
                orig_name: None,
                discr: None,
                fields: vec![FieldMetadata {
                    name: "reason".into(),
                    orig_name: None,
                    ty: Type::String,
                    default: None,
                    docstring: None,
                }],
                docstring: None,
            }],
            discr_type: None,
            non_exhaustive: false,
            docstring: None,
        });
        let files = render(&[item], &[], "test")?;
        assert!(files["proxy.gen.ts"].contains("B.LogSinkError.Failed.new({ reason:"));
        assert!(files["wire.gen.ts"].contains("Failed: {"));
    }

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
    fn pure_function_stays_out_of_worker_dispatch() {
        let item = Metadata::Func(FnMetadata {
            module_path: "test".into(),
            name: "encode_standard".into(),
            orig_name: None,
            is_async: false,
            inputs: vec![],
            return_type: Some(Type::String),
            throws: None,
            checksum: None,
            docstring: Some("@xmtp-pure".into()),
        });
        validate_bridge(std::slice::from_ref(&item))?;
        assert!(operations(&[item]).is_empty());
    }

    // The worker storage lock must name the OPFS directory that the Rust
    // store opens. Only the generator writes it, from the Rust constant.
    #[xmtp_common::test(unwrap_try = true)]
    fn pool_lock_name_comes_from_opfs_directory() {
        let files = render(&[], &[], "test")?;
        let dispatch = &files["dispatch.gen.ts"];
        assert!(
            dispatch.contains(&format!(
                "const STORAGE_POOL = {:?};",
                xmtp_configuration::WASM_VFS_DIRECTORY
            )),
            "dispatch does not hold the OPFS directory"
        );
        assert!(
            dispatch.contains("poolName(decoded[1], STORAGE_POOL)"),
            "dispatch does not pass the OPFS directory to poolName"
        );
        let host = include_str!("../../runtime/ts/bridge/worker/host.ts");
        assert!(
            !host.contains(xmtp_configuration::WASM_VFS_DIRECTORY),
            "host.ts has its own copy of the OPFS directory"
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn worker_function_stays_out_of_bridge() {
        let item = Metadata::Func(FnMetadata {
            module_path: "test".into(),
            name: "storage_requires_worker_restart".into(),
            orig_name: None,
            is_async: false,
            inputs: vec![],
            return_type: Some(Type::Boolean),
            throws: None,
            checksum: None,
            docstring: Some("@xmtp-worker Reports a store.".into()),
        });
        validate_bridge(std::slice::from_ref(&item))?;
        assert!(operations(&[item]).is_empty());
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn rejects_unreviewed_custom_type() {
        let ty = Type::Custom {
            module_path: "test".into(),
            name: "UnknownHostValue".into(),
            builtin: Box::new(Type::String),
        };
        assert!(
            validate_type(&ty)
                .unwrap_err()
                .to_string()
                .contains("UnknownHostValue")
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn listener_id_uses_uint64_on_the_bridge() {
        let ty = Type::Custom {
            module_path: "test".into(),
            name: "ListenerId".into(),
            builtin: Box::new(Type::UInt64),
        };
        assert!(validate_type(&ty).is_ok());
        assert_eq!(shape(&ty), "{ kind: \"value\", type: \"UInt64\" }");
        assert_eq!(decode_expr(&ty, "raw", "session"), "bridgeBigInt(raw)");
        assert_eq!(ts_type(&ty), "B.ListenerId");
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

    fn sync_method(
        name: &str,
        inputs: Vec<uniffi_meta::FnParamMetadata>,
        return_type: Option<Type>,
        throws: Option<Type>,
        docstring: Option<&str>,
    ) -> Metadata {
        Metadata::Method(MethodMetadata {
            module_path: "test".into(),
            self_name: "Group".into(),
            name: name.into(),
            orig_name: None,
            is_async: false,
            inputs,
            return_type,
            throws,
            takes_self_by_arc: true,
            checksum: None,
            docstring: docstring.map(Into::into),
        })
    }

    // An unmarked getter may be a value that never changes, so the error
    // names the marker. Any other synchronous method must become async; the
    // marker would not admit it.
    #[xmtp_common::test(unwrap_try = true)]
    fn sync_method_errors_name_the_fix_for_their_shape() {
        let error = |item: Metadata| validate_bridge(&[item]).unwrap_err().to_string();
        let getter = error(sync_method(
            "connection_state",
            vec![],
            Some(Type::UInt64),
            None,
            None,
        ));
        assert!(
            getter
                .starts_with("Group.connection_state: synchronous getter is not marked immutable"),
            "{getter}"
        );
        assert!(getter.contains("#[sdk(immutable)]"));
        let argument = uniffi_meta::FnParamMetadata::simple("value", Type::UInt64);
        for (name, inputs, output, throws) in [
            ("lookup", vec![argument], Some(Type::UInt64), None),
            ("state", vec![], Some(Type::UInt64), Some(Type::String)),
            ("reset", vec![], None, None),
        ] {
            // Marked or not, a method that is not a getter cannot be a snapshot.
            for docstring in [None, Some("@xmtp-immutable")] {
                let message = error(sync_method(
                    name,
                    inputs.clone(),
                    output.clone(),
                    throws.clone(),
                    docstring,
                ));
                assert!(
                    message.starts_with(&format!("Group.{name}: synchronous worker method")),
                    "{message}"
                );
                assert!(message.ends_with("make it async"), "{message}");
                assert!(!message.contains("immutable"), "{message}");
            }
        }
    }

    // The marker, not a name table, admits a synchronous getter, so a new
    // getter on any object needs no generator change.
    #[xmtp_common::test(unwrap_try = true)]
    fn derives_immutable_getter_from_metadata_marker() {
        let item = sync_method(
            "new_getter",
            vec![],
            Some(Type::UInt64),
            None,
            Some("The value.\n@xmtp-immutable"),
        );
        assert!(validate_bridge(std::slice::from_ref(&item)).is_ok());
        assert!(operations(&[item])[0].immutable);
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
    fn rejects_rust_only_object() {
        let item = Metadata::Object(ObjectMetadata {
            module_path: "test".into(),
            name: "RustOnly".into(),
            orig_name: None,
            remote: false,
            imp: ObjectImpl::Trait(TraitKind::RustOnly),
            docstring: None,
        });
        assert!(
            validate_bridge(&[item])
                .unwrap_err()
                .to_string()
                .contains("Rust-only")
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn rejects_uniffi_display_trait() {
        let method = MethodMetadata {
            module_path: "test".into(),
            self_name: "Group".into(),
            name: "fmt".into(),
            orig_name: None,
            is_async: false,
            inputs: vec![],
            return_type: Some(Type::String),
            throws: None,
            takes_self_by_arc: true,
            checksum: None,
            docstring: None,
        };
        let item = Metadata::UniffiTrait(UniffiTraitMetadata::Display { fmt: method });
        assert!(
            validate_bridge(&[item])
                .unwrap_err()
                .to_string()
                .contains("UniFFI trait")
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn fieldless_enum_is_numeric_on_wire() {
        let item = Metadata::Enum(EnumMetadata {
            module_path: "test".into(),
            name: "Kind".into(),
            orig_name: None,
            shape: EnumShape::Enum,
            remote: false,
            variants: vec!["One", "Two"]
                .into_iter()
                .map(|name| VariantMetadata {
                    name: name.into(),
                    orig_name: None,
                    discr: None,
                    fields: vec![],
                    docstring: None,
                })
                .collect(),
            discr_type: None,
            non_exhaustive: false,
            docstring: None,
        });
        let files = render(&[item], &[], "test")?;
        assert!(files["wire.gen.ts"].contains("export type WireKind = number"));
        assert!(files["wire.gen.ts"].contains("flat: true"));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn generated_client_end_rejects_closed_handle() {
        let object = Metadata::Object(ObjectMetadata {
            module_path: "test".into(),
            name: "Client".into(),
            orig_name: None,
            remote: false,
            imp: ObjectImpl::Struct,
            docstring: None,
        });
        let method = Metadata::Method(MethodMetadata {
            module_path: "test".into(),
            self_name: "Client".into(),
            name: "end".into(),
            orig_name: None,
            is_async: true,
            inputs: vec![],
            return_type: None,
            throws: None,
            takes_self_by_arc: true,
            checksum: None,
            docstring: None,
        });
        let items = [object, method];
        let operations = operations(&items);
        let files = render(&items, &operations, "test")?;
        assert!(files["proxy.gen.ts"].contains("async end(asyncOpts_?:"));
    }

    fn foreign_trait(name: &str, method: &str, is_async: bool) -> [Metadata; 3] {
        let object = Type::Object {
            module_path: "test".into(),
            name: name.into(),
            imp: ObjectImpl::Trait(TraitKind::Both),
        };
        [
            Metadata::Object(ObjectMetadata {
                module_path: "test".into(),
                name: name.into(),
                orig_name: None,
                remote: false,
                imp: ObjectImpl::Trait(TraitKind::Both),
                docstring: None,
            }),
            Metadata::TraitMethod(TraitMethodMetadata {
                module_path: "test".into(),
                trait_name: name.into(),
                index: 0,
                name: method.into(),
                orig_name: None,
                is_async,
                inputs: vec![],
                return_type: Some(Type::String),
                throws: None,
                takes_self_by_arc: true,
                checksum: None,
                docstring: None,
            }),
            Metadata::Func(FnMetadata {
                module_path: "test".into(),
                name: "make".into(),
                orig_name: None,
                is_async: true,
                inputs: vec![],
                return_type: Some(Type::Optional {
                    inner_type: Box::new(object),
                }),
                throws: None,
                checksum: None,
                docstring: None,
            }),
        ]
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn returned_foreign_object_decodes_to_worker_proxy() {
        let items = foreign_trait("Signer", "identity", true);
        validate_bridge(&items)?;
        let operations = operations(&items);
        let files = render(&items, &operations, "test")?;
        let proxy = &files["proxy.gen.ts"];
        assert!(proxy.contains("export class Signer extends RemoteObject implements B.Signer {"));
        assert!(proxy.contains("case \"Signer\": return new Signer(session, handle);"));
        assert!(!proxy.contains("foreign object cannot be returned"));
        assert!(files["dispatch.gen.ts"].contains("\"Signer.identity\": { owner: \"Signer\""));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn rejects_returned_foreign_object_with_sync_method() {
        let items = foreign_trait("LogSink", "log", false);
        assert!(
            validate_bridge(&items)
                .unwrap_err()
                .to_string()
                .contains("LogSink.log: synchronous foreign trait method")
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
        let operations = operations(&items);
        let files = render(&items, &operations, "test")?;
        assert!(files["proxy.gen.ts"].contains("unreadTotal"));
        assert!(files["dispatch.gen.ts"].contains("Group.unreadTotal"));
    }
}
