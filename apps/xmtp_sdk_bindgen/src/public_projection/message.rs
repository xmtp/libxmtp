//! The fields of the public Message, from the `MessageData` record.

use std::fmt::Write as _;

use anyhow::Result;
use uniffi_meta::{Metadata, Type};

use super::{
    camel, convert,
    policy::{cursor_type, is_delivery_cursor},
    public_type,
};
use crate::message_fields::{self, MESSAGE_DATA};

/// The base class of the public `Message`. It lifts each field once, when
/// the message is made; the hand-written `Message` adds its content, reply
/// decoding, and actions. A delivery cursor is `null` when absent; another
/// absent optional field stays unset.
pub(super) fn fields(code: &mut String, items: &[&Metadata]) -> Result<()> {
    let fields = message_fields::fields(items)?;
    let mut declarations = String::new();
    let mut assignments = String::new();
    for field in fields {
        let name = camel(&field.name);
        let value = format!("data.{name}");
        match &field.ty {
            Type::Optional { inner_type } if is_delivery_cursor(MESSAGE_DATA, &name) => {
                writeln!(
                    declarations,
                    "/** The committed delivery position, or null when there is none. */\nreadonly {name}: {} | null;",
                    cursor_type(MESSAGE_DATA, &name, public_type(inner_type))
                )?;
                let lifted = convert(inner_type, &value, false);
                if lifted == value {
                    writeln!(assignments, "this.{name} = {value} ?? null;")?;
                } else {
                    writeln!(
                        assignments,
                        "this.{name} = {value} === undefined ? null : {lifted};"
                    )?;
                }
            }
            Type::Optional { inner_type } => {
                writeln!(
                    declarations,
                    "readonly {name}?: {};",
                    public_type(inner_type)
                )?;
                writeln!(
                    assignments,
                    "if ({value} !== undefined) this.{name} = {};",
                    convert(inner_type, &value, false)
                )?;
            }
            ty => {
                writeln!(
                    declarations,
                    "readonly {name}: {};",
                    cursor_type(MESSAGE_DATA, &name, public_type(ty))
                )?;
                writeln!(assignments, "this.{name} = {};", convert(ty, &value, false))?;
            }
        }
    }
    let unused = if assignments.contains("projection") {
        ""
    } else {
        "void projection;\n"
    };
    writeln!(
        code,
        "/** The fields of a received message. The public Message adds its content and actions. */\nexport abstract class MessageFields {{\n{declarations}protected constructor(data: B.MessageData, projection: ObjectProjection) {{\n{unused}{assignments}}}\n}}"
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use uniffi_meta::Type;

    use super::*;
    use crate::test_metadata::{enum_type, field, optional, record, record_type, sequence};

    fn message_data(fields: Vec<uniffi_meta::FieldMetadata>) -> Metadata {
        let mut all = vec![
            field("client_key", Type::UInt64, None),
            field("content", enum_type("MessageContent"), None),
        ];
        all.extend(fields);
        record(MESSAGE_DATA, all)
    }

    // The public Message keeps its old declarations: a cursor is null when
    // absent, and another absent optional field stays unset.
    #[xmtp_common::test(unwrap_try = true)]
    fn public_fields_lift_each_value_once_and_keep_absent_ones_unset() {
        let timestamp = Type::Custom {
            module_path: "xmtp_sdk".into(),
            name: "Timestamp".into(),
            builtin: Box::new(Type::Int64),
        };
        let items = [message_data(vec![
            field("delivery_cursor", optional(Type::String), None),
            field("expires_at", optional(timestamp), None),
            field("kind", enum_type("MessageKind"), None),
            field("raw_bytes", Type::Bytes, None),
            field("reactions", sequence(record_type("ReactionMessage")), None),
        ])];
        let refs = items.iter().collect::<Vec<_>>();
        let mut code = String::new();
        fields(&mut code, &refs)?;
        assert_eq!(
            code,
            "/** The fields of a received message. The public Message adds its content and actions. */
export abstract class MessageFields {
/** The committed delivery position, or null when there is none. */
readonly deliveryCursor: DeliveryCursor | null;
readonly expiresAt?: Timestamp;
readonly kind: MessageKind;
readonly rawBytes: Uint8Array;
readonly reactions: Array<ReactionMessage>;
protected constructor(data: B.MessageData, projection: ObjectProjection) {
this.deliveryCursor = data.deliveryCursor ?? null;
if (data.expiresAt !== undefined) this.expiresAt = data.expiresAt;
this.kind = liftMessageKind(data.kind, projection);
this.rawBytes = new Uint8Array(data.rawBytes);
this.reactions = data.reactions.map((item) => (liftReactionMessage(item, projection)));
}
}
"
        );
        // A class whose fields need no conversion still names the projection.
        let items = [message_data(vec![field("topic", Type::String, None)])];
        let refs = items.iter().collect::<Vec<_>>();
        let mut code = String::new();
        fields(&mut code, &refs)?;
        assert!(code.contains("{\nvoid projection;\nthis.topic = data.topic;\n}"));
    }
}
