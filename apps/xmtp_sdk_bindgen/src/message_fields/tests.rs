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
    assert!(code.contains("  get id(): MessageData[\"id\"] {\n    return this.data.id;\n  }\n"));
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
    assert!(code.starts_with(
        "// Generated from the MessageData record. Do not edit this output.\nimport Foundation\n"
    ));
    assert!(code.contains(
        "public extension Message {\n    var `id`: MessageId {\n        data.`id`\n    }\n"
    ));
    assert!(
        code.contains(
            "    var `deliveryCursor`: String? {\n        data.`deliveryCursor`\n    }\n"
        )
    );
    assert!(code.contains("    var `reactions`: [ReactionMessage] {\n"));
    assert!(
        code.ends_with("    var `inReplyTo`: ReplyParent? {\n        data.`inReplyTo`\n    }\n}\n")
    );
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
    assert!(
        code.contains(
            "private fun ReplyParent.messageValueEquals(__other: ReplyParent): Boolean ="
        )
    );
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
        code.contains("this.`parts`.fold(1) { __h0, __x0 -> 31 * __h0 + __x0.contentHashCode() }")
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
