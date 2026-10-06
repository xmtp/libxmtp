//! Message field accessors from the `MessageData` record.
//!
//! Each SDK's `Message` wraps one `MessageData`. The generator writes a
//! read accessor for every field, and the hand-written `Message` keeps only
//! what the host does itself: decoding `content` with the client's codecs,
//! reply decoding, and the message actions. A new `MessageData` field needs
//! no edit here or in the runtimes.
//!
//! - TypeScript: `message-fields.gen.ts`, the getters of the binding message
//!   that the Node and browser host classes extend. The public `Message`
//!   extends the `MessageFields` class of the public projection instead.
//! - Kotlin: `runtime/MessageFields.kt`, the base class with the accessors
//!   and value equality over every field.
//! - Swift: `runtime/MessageFields.swift`, an extension of `Message`.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
    fs,
};

use anyhow::{Context, Result, bail};
use camino::Utf8Path;
use heck::{ToLowerCamelCase, ToUpperCamelCase};
use uniffi_meta::{EnumMetadata, FieldMetadata, Metadata, MetadataGroupMap, RecordMetadata, Type};

use crate::kotlin_records::byte_field;
use crate::redaction::field_name;

/// The record that a `Message` wraps.
pub(crate) const MESSAGE_DATA: &str = "MessageData";
/// Fields that the host Message reads itself: the key of the client that
/// returned the message, and the content that the client's codecs decode.
const HOST_FIELDS: &[&str] = &["client_key", "content"];

/// The `MessageData` fields that get an accessor.
pub(crate) fn fields<'a>(items: &[&'a Metadata]) -> Result<Vec<&'a FieldMetadata>> {
    let record = items
        .iter()
        .find_map(|item| match item {
            Metadata::Record(record) if record.name == MESSAGE_DATA => Some(record),
            _ => None,
        })
        .with_context(|| format!("the metadata has no {MESSAGE_DATA} record"))?;
    for name in HOST_FIELDS {
        if !record.fields.iter().any(|field| field.name == *name) {
            bail!("{MESSAGE_DATA} has no `{name}`, which the host Message reads itself");
        }
    }
    Ok(record
        .fields
        .iter()
        .filter(|field| !HOST_FIELDS.contains(&field.name.as_str()))
        .collect())
}

fn items(groups: &MetadataGroupMap) -> Vec<&Metadata> {
    groups.values().flat_map(|group| &group.items).collect()
}

/// Write `message-fields.gen.ts` into a TypeScript tree.
pub(crate) fn generate_typescript(groups: &MetadataGroupMap, out: &Utf8Path) -> Result<()> {
    let items = items(groups);
    let fields = fields(&items)?;
    let name = "message-fields.gen.ts";
    let code = typescript(&fields);
    fs::write(out.join(name), crate::format::typescript(name, &code)?)?;
    Ok(())
}

/// `runtime/MessageFields.swift`, from the generated Swift binding.
pub(crate) fn generate_swift(groups: &MetadataGroupMap, binding: &str) -> Result<String> {
    let items = items(groups);
    swift(&fields(&items)?, binding)
}

/// `runtime/MessageFields.kt`.
pub(crate) fn generate_kotlin(groups: &MetadataGroupMap) -> Result<String> {
    let items = items(groups);
    kotlin(&fields(&items)?, &items)
}

/// TypeScript: the getters of a binding message. A delivery cursor, as the
/// public projection policy names it, reads as `null` when absent.
fn typescript(fields: &[&FieldMetadata]) -> String {
    let mut code = String::from(
        "// Generated from the MessageData record. Do not edit this output.\nimport type { MessageData } from \"./xmtp_sdk\";\n\n/** The fields of a binding message. The host Message adds its content and actions. */\nexport abstract class MessageFields {\n  constructor(readonly data: MessageData) {}\n",
    );
    for field in fields {
        let name = field.name.to_lower_camel_case();
        let is_cursor = crate::public_projection::is_delivery_cursor(MESSAGE_DATA, &name)
            && matches!(field.ty, Type::Optional { .. });
        let (ty, value) = if is_cursor {
            (
                "string | null".to_owned(),
                format!("this.data.{name} ?? null"),
            )
        } else {
            (
                format!("MessageData[\"{name}\"]"),
                format!("this.data.{name}"),
            )
        };
        let doc = if is_cursor {
            "\n  /** The committed delivery position, or null when there is none. */"
        } else {
            ""
        };
        let _ = write!(
            code,
            "{doc}\n  get {name}(): {ty} {{\n    return {value};\n  }}\n"
        );
    }
    code.push_str("}\n");
    code
}

/// Swift: an accessor per field, with the type that the binding's
/// `MessageData` struct declares.
fn swift(fields: &[&FieldMetadata], binding: &str) -> Result<String> {
    let body = binding
        .split_once(&format!("\npublic struct {MESSAGE_DATA}"))
        .with_context(|| format!("the Swift binding has no {MESSAGE_DATA} struct"))?
        .1
        .split_once("\n\n")
        .with_context(|| format!("the Swift {MESSAGE_DATA} struct has no end"))?
        .0;
    let declared = body
        .lines()
        .filter_map(|line| line.strip_prefix("    public var "))
        .filter_map(|line| line.split_once(": "))
        .map(|(name, ty)| (name.trim_matches('`'), ty.trim()))
        .collect::<BTreeMap<_, _>>();
    let mut code = String::from(
        "// Generated from the MessageData record. Do not edit this output.\nimport Foundation\n\n/// The fields of a received message. `Message` adds its decoded content and actions.\npublic extension Message {\n",
    );
    for field in fields {
        let name = field.name.to_lower_camel_case();
        let ty = declared
            .get(name.as_str())
            .with_context(|| format!("the Swift {MESSAGE_DATA} struct has no `{name}` field"))?;
        writeln!(
            code,
            "    var `{name}`: {ty} {{\n        data.`{name}`\n    }}\n"
        )?;
    }
    let code = code.trim_end().to_owned();
    Ok(format!("{code}\n}}\n"))
}

/// Kotlin: the accessor base class of `Message`, with value equality over
/// every `MessageData` field.
fn kotlin(fields: &[&FieldMetadata], items: &[&Metadata]) -> Result<String> {
    let mut equality = KotlinEquality::new(items);
    let data = MESSAGE_DATA.to_upper_camel_case();
    // MessageData compares every field, the host ones too.
    equality.pending.push(data.clone());
    let helpers = equality.helpers()?;
    let mut code = format!(
        "// Generated from the MessageData record. Do not edit this output.\npackage {}\n\n/**\n * The fields of a received message, and value equality over all of its data.\n * [Message] adds its decoded content and actions.\n */\nabstract class MessageFields internal constructor(\n    val data: {data},\n) {{\n",
        crate::client_statics::KOTLIN_PACKAGE
    );
    for field in fields {
        let name = field.name.to_lower_camel_case();
        writeln!(code, "    val `{name}` get() = data.`{name}`\n")?;
    }
    writeln!(
        code,
        "    final override fun equals(other: Any?): Boolean = other is MessageFields && data.messageValueEquals(other.data)\n\n    final override fun hashCode(): Int = data.messageValueHash()\n}}\n"
    )?;
    code.push_str(&helpers);
    Ok(code)
}

/// The helpers that a generated comparison calls, written only when used.
const OPTIONAL_EQUALS: &str = "private inline fun <T : Any> messageOptionalEquals(\n    a: T?,\n    b: T?,\n    equal: (T, T) -> Boolean,\n): Boolean = if (a == null || b == null) a == null && b == null else equal(a, b)\n";
const LIST_EQUALS: &str = "private inline fun <T> messageListEquals(\n    a: List<T>,\n    b: List<T>,\n    equal: (T, T) -> Boolean,\n): Boolean = a.size == b.size && a.indices.all { equal(a[it], b[it]) }\n";
const MAP_EQUALS: &str = "private inline fun <K, V> messageMapEquals(\n    a: Map<K, V>,\n    b: Map<K, V>,\n    equal: (V, V) -> Boolean,\n): Boolean = a.size == b.size && a.all { (key, value) -> b.containsKey(key) && equal(value, b.getValue(key)) }\n";

/// Kotlin value equality over UniFFI types. A generated data class compares
/// a `ByteArray` by reference. `kotlin_records` gives a record with byte
/// fields value equality, but an enum variant keeps reference equality for
/// its bytes. A type whose `==` is not value equality gets a generated
/// comparison and hash; every other type uses `==` and `hashCode`.
struct KotlinEquality<'a> {
    records: BTreeMap<&'a str, &'a RecordMetadata>,
    enums: BTreeMap<&'a str, &'a EnumMetadata>,
    /// Named types to write a comparison for, in order of first use.
    pending: Vec<String>,
    written: BTreeSet<String>,
    optional: bool,
    list: bool,
    map: bool,
}

impl<'a> KotlinEquality<'a> {
    fn new(items: &[&'a Metadata]) -> Self {
        let mut records = BTreeMap::new();
        let mut enums = BTreeMap::new();
        for item in items {
            match item {
                Metadata::Record(record) => {
                    records.insert(record.name.as_str(), record);
                }
                Metadata::Enum(value) => {
                    enums.insert(value.name.as_str(), value);
                }
                _ => {}
            }
        }
        Self {
            records,
            enums,
            pending: Vec::new(),
            written: BTreeSet::new(),
            optional: false,
            list: false,
            map: false,
        }
    }

    /// Whether `==` on the type is not value equality.
    fn needs(&self, ty: &Type) -> bool {
        self.needs_within(ty, &mut BTreeSet::new())
    }

    fn needs_within(&self, ty: &Type, seen: &mut BTreeSet<String>) -> bool {
        match ty {
            Type::Bytes => true,
            Type::Optional { inner_type }
            | Type::Sequence { inner_type }
            | Type::Set { inner_type }
            | Type::Box { inner_type } => self.needs_within(inner_type, seen),
            Type::Map {
                key_type,
                value_type,
            } => self.needs_within(key_type, seen) || self.needs_within(value_type, seen),
            // The host Message compares by value itself.
            Type::Custom { name, .. } if name == "Message" => false,
            Type::Custom { builtin, .. } => self.needs_within(builtin, seen),
            Type::Record { name, .. } => {
                if !seen.insert(name.clone()) {
                    return false;
                }
                self.records.get(name.as_str()).is_some_and(|record| {
                    // A Kotlin class without fields compares by reference.
                    record.fields.is_empty()
                        || record.fields.iter().any(|field| {
                            !byte_field(&field.ty) && self.needs_within(&field.ty, seen)
                        })
                })
            }
            Type::Enum { name, .. } => {
                if !seen.insert(name.clone()) {
                    return false;
                }
                self.enums.get(name.as_str()).is_some_and(|value| {
                    value.variants.iter().any(|variant| {
                        variant
                            .fields
                            .iter()
                            .any(|field| self.needs_within(&field.ty, seen))
                    })
                })
            }
            _ => false,
        }
    }

    /// The comparison of `a` and `b`. `depth` names lambda parameters
    /// apart.
    fn equals(&mut self, ty: &Type, a: &str, b: &str, depth: usize) -> Result<String> {
        if !self.needs(ty) {
            return Ok(format!("{a} == {b}"));
        }
        let (x, y) = (format!("__x{depth}"), format!("__y{depth}"));
        Ok(match ty {
            Type::Bytes => format!("{a}.contentEquals({b})"),
            Type::Box { inner_type } => self.equals(inner_type, a, b, depth)?,
            Type::Custom { builtin, .. } => self.equals(builtin, a, b, depth)?,
            Type::Optional { inner_type } => {
                self.optional = true;
                let inner = self.equals(inner_type, &x, &y, depth + 1)?;
                format!("messageOptionalEquals({a}, {b}) {{ {x}, {y} -> {inner} }}")
            }
            Type::Sequence { inner_type } => {
                self.list = true;
                let inner = self.equals(inner_type, &x, &y, depth + 1)?;
                format!("messageListEquals({a}, {b}) {{ {x}, {y} -> {inner} }}")
            }
            Type::Map {
                key_type,
                value_type,
            } if !self.needs(key_type) => {
                self.map = true;
                let inner = self.equals(value_type, &x, &y, depth + 1)?;
                format!("messageMapEquals({a}, {b}) {{ {x}, {y} -> {inner} }}")
            }
            Type::Record { name, .. } | Type::Enum { name, .. } => {
                self.pending.push(name.to_upper_camel_case());
                format!("{a}.messageValueEquals({b})")
            }
            _ => bail!("{MESSAGE_DATA}: no Kotlin value equality for {ty:?}"),
        })
    }

    fn hash(&mut self, ty: &Type, value: &str, depth: usize) -> Result<String> {
        if !self.needs(ty) {
            return Ok(format!("{value}.hashCode()"));
        }
        let x = format!("__x{depth}");
        Ok(match ty {
            Type::Bytes => format!("{value}.contentHashCode()"),
            Type::Box { inner_type } => self.hash(inner_type, value, depth)?,
            Type::Custom { builtin, .. } => self.hash(builtin, value, depth)?,
            Type::Optional { inner_type } => {
                let inner = self.hash(inner_type, &x, depth + 1)?;
                format!("({value}?.let {{ {x} -> {inner} }} ?: 0)")
            }
            Type::Sequence { inner_type } => {
                let inner = self.hash(inner_type, &x, depth + 1)?;
                let h = format!("__h{depth}");
                format!("{value}.fold(1) {{ {h}, {x} -> 31 * {h} + {inner} }}")
            }
            Type::Map { value_type, .. } => {
                let inner = self.hash(value_type, &format!("{x}.value"), depth + 1)?;
                format!("{value}.entries.sumOf {{ {x} -> {x}.key.hashCode() xor {inner} }}")
            }
            Type::Record { name, .. } | Type::Enum { name, .. } => {
                self.pending.push(name.to_upper_camel_case());
                format!("{value}.messageValueHash()")
            }
            _ => bail!("{MESSAGE_DATA}: no Kotlin value hash for {ty:?}"),
        })
    }

    /// The comparison and hash of the fields of one record or variant,
    /// which `this` and `__other` hold. Each field is qualified, and the
    /// generated names start with `__`, so no field shadows them.
    fn members(&mut self, fields: &[FieldMetadata]) -> Result<(String, String)> {
        let mut equals = Vec::new();
        let mut hash = String::new();
        for (index, field) in fields.iter().enumerate() {
            let name = field_name(field, index);
            let (a, b) = (format!("this.`{name}`"), format!("__other.`{name}`"));
            // `kotlin_records` gives a record's byte field value equality.
            equals.push(self.equals(&field.ty, &a, &b, 0)?);
            let value = self.hash(&field.ty, &a, 0)?;
            if index == 0 {
                hash = format!("var __result = {value}\n");
            } else {
                let _ = writeln!(hash, "__result = 31 * __result + {value}");
            }
        }
        Ok((equals.join(" &&\n        "), hash))
    }

    /// Every pending named type's comparison and hash, then the helpers they
    /// call.
    fn helpers(&mut self) -> Result<String> {
        let mut code = String::new();
        while let Some(class) = self.pending.pop() {
            if !self.written.insert(class.clone()) {
                continue;
            }
            if let Some(record) = self
                .records
                .values()
                .find(|record| record.name.to_upper_camel_case() == class)
                .copied()
            {
                if record.fields.is_empty() {
                    writeln!(
                        code,
                        "private fun {class}.messageValueEquals(__other: {class}): Boolean = true\n\nprivate fun {class}.messageValueHash(): Int = 0\n"
                    )?;
                    continue;
                }
                let (equals, hash) = self.members(&record.fields)?;
                let hash = hash.replace('\n', "\n    ");
                writeln!(
                    code,
                    "private fun {class}.messageValueEquals(__other: {class}): Boolean =\n    {equals}\n\nprivate fun {class}.messageValueHash(): Int {{\n    {hash}return __result\n}}\n"
                )?;
                continue;
            }
            let value = self
                .enums
                .values()
                .find(|value| value.name.to_upper_camel_case() == class)
                .copied()
                .with_context(|| format!("{MESSAGE_DATA}: no record or enum {class}"))?;
            let mut equals = String::new();
            let mut hash = String::new();
            let mut covered = 0;
            for variant in &value.variants {
                if !variant.fields.iter().any(|field| self.needs(&field.ty)) {
                    continue;
                }
                covered += 1;
                let name = format!("{class}.{}", variant.name.to_upper_camel_case());
                let (fields, fields_hash) = self.members(&variant.fields)?;
                let fields = fields.replace("\n        ", "\n            ");
                let _ = writeln!(
                    equals,
                    "        is {name} -> __other is {name} &&\n            {fields}"
                );
                let fields_hash = fields_hash.replace('\n', "\n            ");
                let _ = writeln!(
                    hash,
                    "        is {name} -> {{\n            {fields_hash}__result\n        }}"
                );
            }
            // Every other variant compares and hashes as its class does.
            let (other_equals, other_hash) = if covered < value.variants.len() {
                (
                    "        else -> this == __other\n",
                    "        else -> hashCode()\n",
                )
            } else {
                ("", "")
            };
            writeln!(
                code,
                "private fun {class}.messageValueEquals(__other: {class}): Boolean =\n    when (this) {{\n{equals}{other_equals}    }}\n\nprivate fun {class}.messageValueHash(): Int =\n    when (this) {{\n{hash}{other_hash}    }}\n"
            )?;
        }
        for (used, helper) in [
            (self.optional, OPTIONAL_EQUALS),
            (self.list, LIST_EQUALS),
            (self.map, MAP_EQUALS),
        ] {
            if used {
                writeln!(code, "{helper}")?;
            }
        }
        Ok(code)
    }
}

#[cfg(test)]
mod tests {
    use uniffi_meta::Type;

    use super::*;
    use crate::test_metadata::{
        enum_type, enumeration, field, groups, optional, record, record_type, sequence, variant,
    };

    fn custom(name: &str, builtin: Type) -> Type {
        Type::Custom {
            module_path: "xmtp_sdk".into(),
            name: name.into(),
            builtin: Box::new(builtin),
        }
    }

    /// A small façade: the message record, a content enum whose variants
    /// hold bytes, and a reply parent that holds such an enum.
    fn metadata() -> MetadataGroupMap {
        groups(vec![
            record(
                MESSAGE_DATA,
                vec![
                    field("id", custom("MessageId", Type::String), None),
                    field("client_key", Type::UInt64, None),
                    field("delivery_cursor", optional(Type::String), None),
                    field("raw_bytes", Type::Bytes, None),
                    field("encoded", optional(record_type("EncodedContent")), None),
                    field("content", enum_type("MessageContent"), None),
                    field("reactions", sequence(record_type("Reaction")), None),
                    field("in_reply_to", optional(record_type("ReplyParent")), None),
                ],
            ),
            // A record with a byte field compares by value already.
            record(
                "EncodedContent",
                vec![
                    field("fallback", optional(Type::String), None),
                    field("content", Type::Bytes, None),
                ],
            ),
            record("Reaction", vec![field("emoji", Type::String, None)]),
            record(
                "ReplyParent",
                vec![
                    field("id", custom("MessageId", Type::String), None),
                    field("content", enum_type("MessageBody"), None),
                ],
            ),
            enumeration(
                "MessageContent",
                vec![
                    variant("Text", None, vec![field("", Type::String, None)]),
                    variant("ReadReceipt", None, vec![]),
                    variant(
                        "Custom",
                        None,
                        vec![
                            field("encoded", record_type("EncodedContent"), None),
                            field("raw_bytes", Type::Bytes, None),
                        ],
                    ),
                ],
            ),
            enumeration(
                "MessageBody",
                vec![variant(
                    "Unknown",
                    None,
                    vec![
                        field("raw_bytes", Type::Bytes, None),
                        field("error", Type::String, None),
                    ],
                )],
            ),
        ])
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn accessors_skip_the_fields_the_host_reads() {
        let groups = metadata();
        let items = items(&groups);
        let names = fields(&items)?
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            [
                "id",
                "delivery_cursor",
                "raw_bytes",
                "encoded",
                "reactions",
                "in_reply_to"
            ]
        );
        // A renamed host field stops generation instead of becoming public.
        let renamed = groups_without(&groups, "client_key");
        let error = fields(&items_of(&renamed)).unwrap_err().to_string();
        assert!(error.contains("has no `client_key`"), "{error}");
    }

    fn groups_without(groups: &MetadataGroupMap, name: &str) -> Vec<Metadata> {
        items(groups)
            .into_iter()
            .map(|item| match item {
                Metadata::Record(value) if value.name == MESSAGE_DATA => {
                    let mut value = value.clone();
                    value.fields.retain(|field| field.name != name);
                    Metadata::Record(value)
                }
                item => item.clone(),
            })
            .collect()
    }

    fn items_of(items: &[Metadata]) -> Vec<&Metadata> {
        items.iter().collect()
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn typescript_getters_read_each_field_and_a_missing_cursor_as_null() {
        let groups = metadata();
        let items = items(&groups);
        let code = typescript(&fields(&items)?);
        assert!(code.contains(
            "export abstract class MessageFields {\n  constructor(readonly data: MessageData) {}\n"
        ));
        assert!(
            code.contains("  get id(): MessageData[\"id\"] {\n    return this.data.id;\n  }\n")
        );
        assert!(code.contains(
            "  /** The committed delivery position, or null when there is none. */\n  get deliveryCursor(): string | null {\n    return this.data.deliveryCursor ?? null;\n  }\n"
        ));
        assert!(code.contains("  get inReplyTo(): MessageData[\"inReplyTo\"] {"));
        assert!(!code.contains("clientKey"));
        assert!(!code.contains("get content"));
        assert_eq!(code.matches(" get ").count(), 6);
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn swift_accessors_take_the_types_of_the_binding_struct() {
        let groups = metadata();
        let binding = "\
public struct OtherData {
    public var id: Int
}
public struct MessageData: Equatable, Hashable {
    public var id: MessageId
    public var clientKey: UInt64
    public var deliveryCursor: String?
    /**
     * The bytes.
     */
    public var rawBytes: Data
    public var encoded: EncodedContent?
    public var content: MessageContent
    public var reactions: [ReactionMessage]
    public var inReplyTo: ReplyParent?

    // Default memberwise initializers are never public by default, so we
";
        let code = generate_swift(&groups, binding)?;
        assert!(code.starts_with("// Generated from the MessageData record. Do not edit this output.\nimport Foundation\n"));
        assert!(code.contains(
            "public extension Message {\n    var `id`: MessageId {\n        data.`id`\n    }\n"
        ));
        assert!(code.contains(
            "    var `deliveryCursor`: String? {\n        data.`deliveryCursor`\n    }\n"
        ));
        assert!(code.contains("    var `reactions`: [ReactionMessage] {\n"));
        assert!(code.ends_with(
            "    var `inReplyTo`: ReplyParent? {\n        data.`inReplyTo`\n    }\n}\n"
        ));
        assert!(!code.contains("clientKey") && !code.contains("`content`"));
        let missing = binding.replace("    public var rawBytes: Data\n", "");
        let error = generate_swift(&groups, &missing).unwrap_err().to_string();
        assert!(error.contains("has no `rawBytes` field"), "{error}");
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn kotlin_equality_compares_bytes_and_variant_bytes_by_value() {
        let code = generate_kotlin(&metadata())?;
        assert!(code.contains(
            "abstract class MessageFields internal constructor(\n    val data: MessageData,\n) {\n    val `id` get() = data.`id`\n"
        ));
        assert!(!code.contains("val `clientKey`") && !code.contains("val `content`"));
        assert!(code.contains(
            "    final override fun equals(other: Any?): Boolean = other is MessageFields && data.messageValueEquals(other.data)\n"
        ));
        // Every MessageData field takes part in equality, the host ones too.
        assert!(code.contains(
            "private fun MessageData.messageValueEquals(__other: MessageData): Boolean =
    this.`id` == __other.`id` &&
        this.`clientKey` == __other.`clientKey` &&
        this.`deliveryCursor` == __other.`deliveryCursor` &&
        this.`rawBytes`.contentEquals(__other.`rawBytes`) &&
        this.`encoded` == __other.`encoded` &&
        this.`content`.messageValueEquals(__other.`content`) &&
        this.`reactions` == __other.`reactions` &&
        messageOptionalEquals(this.`inReplyTo`, __other.`inReplyTo`) { __x0, __y0 -> __x0.messageValueEquals(__y0) }
"
        ));
        assert!(code.contains(
            "private fun MessageData.messageValueHash(): Int {
    var __result = this.`id`.hashCode()
    __result = 31 * __result + this.`clientKey`.hashCode()
    __result = 31 * __result + this.`deliveryCursor`.hashCode()
    __result = 31 * __result + this.`rawBytes`.contentHashCode()
    __result = 31 * __result + this.`encoded`.hashCode()
    __result = 31 * __result + this.`content`.messageValueHash()
    __result = 31 * __result + this.`reactions`.hashCode()
    __result = 31 * __result + (this.`inReplyTo`?.let { __x0 -> __x0.messageValueHash() } ?: 0)
    return __result
}
"
        ));
        // A variant's bytes compare by value; another variant as its class does.
        assert!(code.contains(
            "private fun MessageContent.messageValueEquals(__other: MessageContent): Boolean =
    when (this) {
        is MessageContent.Custom -> __other is MessageContent.Custom &&
            this.`encoded` == __other.`encoded` &&
            this.`rawBytes`.contentEquals(__other.`rawBytes`)
        else -> this == __other
    }
"
        ));
        // Every variant of MessageBody needs the comparison: no `else`.
        assert!(code.contains(
            "private fun MessageBody.messageValueHash(): Int =
    when (this) {
        is MessageBody.Unknown -> {
            var __result = this.`rawBytes`.contentHashCode()
            __result = 31 * __result + this.`error`.hashCode()
            __result
        }
    }
"
        ));
        assert!(code.contains(
            "private fun ReplyParent.messageValueEquals(__other: ReplyParent): Boolean ="
        ));
        // EncodedContent and Reaction compare as their classes do.
        assert!(!code.contains("EncodedContent.messageValueEquals"));
        assert!(!code.contains("Reaction.messageValueEquals"));
        assert!(code.contains("private inline fun <T : Any> messageOptionalEquals("));
        assert!(!code.contains("messageListEquals(") && !code.contains("messageMapEquals("));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn kotlin_equality_reaches_into_lists_and_stops_on_a_set_of_bytes() {
        let with = |ty: Type| {
            groups(vec![record(
                MESSAGE_DATA,
                vec![
                    field("client_key", Type::UInt64, None),
                    field("content", Type::String, None),
                    field("parts", ty, None),
                ],
            )])
        };
        let code = generate_kotlin(&with(sequence(Type::Bytes)))?;
        assert!(code.contains(
            "messageListEquals(this.`parts`, __other.`parts`) { __x0, __y0 -> __x0.contentEquals(__y0) }"
        ));
        assert!(
            code.contains(
                "this.`parts`.fold(1) { __h0, __x0 -> 31 * __h0 + __x0.contentHashCode() }"
            )
        );
        assert!(code.contains("private inline fun <T> messageListEquals("));
        let set = Type::Set {
            inner_type: Box::new(Type::Bytes),
        };
        let error = generate_kotlin(&with(set)).unwrap_err().to_string();
        assert!(error.contains("no Kotlin value equality"), "{error}");
    }

    // A field may share a name with the comparison's parameter or the hash's
    // local: each field is qualified, and the generated names start with __.
    #[xmtp_common::test(unwrap_try = true)]
    fn kotlin_equality_names_cannot_shadow_a_field() {
        let code = generate_kotlin(&groups(vec![
            record(
                MESSAGE_DATA,
                vec![
                    field("client_key", Type::UInt64, None),
                    field("content", Type::String, None),
                    field("in_reply_to", optional(record_type("ReplyParent")), None),
                ],
            ),
            record(
                "ReplyParent",
                vec![
                    field("other", enum_type("ReplyBody"), None),
                    field("result", Type::String, None),
                ],
            ),
            enumeration(
                "ReplyBody",
                vec![variant(
                    "Raw",
                    None,
                    vec![
                        field("other", Type::Bytes, None),
                        field("result", Type::String, None),
                    ],
                )],
            ),
        ]))?;
        assert!(code.contains(
            "private fun ReplyParent.messageValueEquals(__other: ReplyParent): Boolean =
    this.`other`.messageValueEquals(__other.`other`) &&
        this.`result` == __other.`result`
"
        ));
        assert!(code.contains(
            "private fun ReplyParent.messageValueHash(): Int {
    var __result = this.`other`.messageValueHash()
    __result = 31 * __result + this.`result`.hashCode()
    return __result
}
"
        ));
        assert!(code.contains(
            "        is ReplyBody.Raw -> __other is ReplyBody.Raw &&
            this.`other`.contentEquals(__other.`other`) &&
            this.`result` == __other.`result`
"
        ));
        assert!(code.contains(
            "        is ReplyBody.Raw -> {
            var __result = this.`other`.contentHashCode()
            __result = 31 * __result + this.`result`.hashCode()
            __result
        }
"
        ));
    }
}
