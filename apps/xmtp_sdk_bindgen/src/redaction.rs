//! Diagnostic text for records and enums with redacted fields.
//!
//! `#[sdk(redact)]` marks a field whose value stays out of Kotlin `toString`
//! and Swift `description`, and `#[sdk(redact = "key")]` hides one key of a
//! string map. Kotlin data classes and Swift structs print every field by
//! default, so the generator writes the text from the record's metadata.
//! Kotlin prints the length of a byte field, so a payload never reaches
//! diagnostic text; Swift prints a byte count by default.

use std::fmt::Write as _;

use anyhow::{Result, bail};
use heck::{ToLowerCamelCase, ToUpperCamelCase};
use uniffi_meta::{EnumMetadata, FieldMetadata, Metadata, MetadataGroupMap, RecordMetadata, Type};

use crate::markers::{self, Redaction};

const REDACTED: &str = "<redacted>";

/// Whether any field hides a value.
pub(crate) fn redacted(fields: &[FieldMetadata]) -> bool {
    fields
        .iter()
        .any(|field| markers::redaction(field).is_some())
}

/// A map key that is safe inside a Kotlin or Swift string literal. The macro
/// admits the same characters; this checks the metadata again before the key
/// reaches generated code.
fn safe_key(key: &str) -> bool {
    !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
}

/// The field's redaction. A key names one entry of a string map.
fn checked(owner: &str, field: &FieldMetadata) -> Result<Option<Redaction>> {
    let redaction = markers::redaction(field);
    if let Some(Redaction::Key(key)) = &redaction {
        let string_map = matches!(&field.ty, Type::Map { key_type, value_type }
            if **key_type == Type::String && **value_type == Type::String);
        if !string_map {
            bail!(
                "{owner}.{}: redact = \"key\" needs a map with string keys and values",
                field.name
            );
        }
        if !safe_key(key) {
            bail!(
                "{owner}.{}: redact key {key:?} must be letters, digits, `_`, `.`, or `-`",
                field.name
            );
        }
    }
    Ok(redaction)
}

/// The binding name of a field. A tuple variant's fields have no name in the
/// metadata, and UniFFI names them `v1`, `v2`, … in Kotlin and Swift.
fn field_name(field: &FieldMetadata, index: usize) -> String {
    if field.name.is_empty() {
        format!("v{}", index + 1)
    } else {
        field.name.to_lower_camel_case()
    }
}

/// Kotlin: `Name(a=${`a`}, secret=<redacted>, keyBytes=${`key`.size})`.
/// `owner` is the Kotlin class name.
pub(crate) fn kotlin_text(owner: &str, fields: &[FieldMetadata]) -> Result<String> {
    let mut parts = Vec::with_capacity(fields.len());
    for (index, field) in fields.iter().enumerate() {
        let name = field_name(field, index);
        parts.push(match checked(owner, field)? {
            Some(Redaction::Whole) => format!("{name}={REDACTED}"),
            Some(Redaction::Key(key)) => format!(
                "{name}=${{`{name}`.mapValues {{ entry -> if (entry.key == \"{key}\") \"{REDACTED}\" else entry.value }}}}"
            ),
            None => match &field.ty {
                Type::Bytes => format!("{name}Bytes=${{`{name}`.size}}"),
                Type::Optional { inner_type } if **inner_type == Type::Bytes => {
                    format!("{name}Bytes=${{`{name}`?.size}}")
                }
                _ => format!("{name}=${{`{name}`}}"),
            },
        });
    }
    Ok(format!("{owner}({})", parts.join(", ")))
}

/// Swift: the interpolated fields of one record or enum case. `values` are
/// the Swift expressions that hold the fields. A tuple variant's fields have
/// no label in Swift, so they print without one.
fn swift_text(owner: &str, fields: &[FieldMetadata], values: &[String]) -> Result<String> {
    let mut parts = Vec::with_capacity(fields.len());
    for (field, value) in fields.iter().zip(values) {
        let label = if field.name.is_empty() {
            String::new()
        } else {
            format!("{}: ", field.name.to_lower_camel_case())
        };
        parts.push(match checked(owner, field)? {
            Some(Redaction::Whole) => format!("{label}{REDACTED}"),
            Some(Redaction::Key(key)) => {
                format!("{label}\\(String(reflecting: sdkRedacted({value}, key: \"{key}\")))")
            }
            None => format!("{label}\\(String(reflecting: {value}))"),
        });
    }
    Ok(format!("{owner}({})", parts.join(", ")))
}

fn swift_extension(code: &mut String, name: &str, body: &str) -> Result<()> {
    writeln!(
        code,
        "extension {name}: CustomStringConvertible, CustomDebugStringConvertible {{\n    public var description: String {{\n{body}    }}\n\n    public var debugDescription: String {{\n        description\n    }}\n}}\n"
    )?;
    Ok(())
}

fn swift_record(code: &mut String, record: &RecordMetadata) -> Result<()> {
    // UniFFI's Swift type name.
    let name = record.name.to_upper_camel_case();
    let values = record
        .fields
        .iter()
        .enumerate()
        .map(|(index, field)| format!("self.`{}`", field_name(field, index)))
        .collect::<Vec<_>>();
    let text = swift_text(&name, &record.fields, &values)?;
    swift_extension(code, &name, &format!("        return \"{text}\"\n"))
}

fn swift_enum(code: &mut String, value: &EnumMetadata) -> Result<()> {
    let name = value.name.to_upper_camel_case();
    let mut body = String::from("        switch self {\n");
    for variant in &value.variants {
        let case = variant.name.to_lower_camel_case();
        let owner = format!("{name}.{case}");
        if variant.fields.is_empty() {
            writeln!(body, "        case .{case}: return \"{owner}\"")?;
            continue;
        }
        let values = variant
            .fields
            .iter()
            .enumerate()
            .map(|(index, field)| format!("`{}`", field_name(field, index)))
            .collect::<Vec<_>>();
        let text = swift_text(&owner, &variant.fields, &values)?;
        // A redacted value is never bound, so it cannot reach the text.
        let bindings = variant
            .fields
            .iter()
            .zip(&values)
            .map(|(field, value)| match markers::redaction(field) {
                Some(Redaction::Whole) => "_",
                _ => value.as_str(),
            })
            .collect::<Vec<_>>();
        if bindings.iter().all(|binding| *binding == "_") {
            writeln!(body, "        case .{case}: return \"{text}\"")?;
        } else {
            writeln!(
                body,
                "        case let .{case}({}): return \"{text}\"",
                bindings.join(", ")
            )?;
        }
    }
    body.push_str("        }\n");
    swift_extension(code, &name, &body)
}

/// Swift descriptions for every record and enum with a redacted field.
pub(crate) fn swift(groups: &MetadataGroupMap) -> Result<String> {
    let mut code = String::from(
        "// Generated from #[sdk(redact)] record fields. Do not edit this output.\nimport Foundation\n\n",
    );
    let mut keys = false;
    for item in groups.values().flat_map(|group| &group.items) {
        let fields: Vec<&FieldMetadata> = match item {
            Metadata::Record(record) if redacted(&record.fields) => {
                swift_record(&mut code, record)?;
                record.fields.iter().collect()
            }
            Metadata::Enum(value)
                if value
                    .variants
                    .iter()
                    .any(|variant| redacted(&variant.fields)) =>
            {
                swift_enum(&mut code, value)?;
                value
                    .variants
                    .iter()
                    .flat_map(|variant| &variant.fields)
                    .collect()
            }
            _ => continue,
        };
        keys |= fields
            .iter()
            .any(|field| matches!(markers::redaction(field), Some(Redaction::Key(_))));
    }
    if keys {
        code.push_str("private func sdkRedacted(_ map: [String: String], key: String) -> [String: String] {\n    var map = map\n    if map[key] != nil {\n        map[key] = \"<redacted>\"\n    }\n    return map\n}\n");
    }
    Ok(code)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_metadata::{enumeration, field, groups, optional, record, variant};

    fn string_map() -> Type {
        Type::Map {
            key_type: Box::new(Type::String),
            value_type: Box::new(Type::String),
        }
    }

    // A redacted value, a redacted map key, and byte contents never reach the
    // text; every other field prints its value.
    #[xmtp_common::test(unwrap_try = true)]
    fn kotlin_text_redacts_fields_and_keys_and_prints_byte_lengths() {
        let fields = [
            field("name", optional(Type::String), None),
            field("value", Type::String, Some("@xmtp-redact")),
            field(
                "parameters",
                string_map(),
                Some("Parameters.\n@xmtp-redact=secret"),
            ),
            field("content", Type::Bytes, None),
            field("salt", optional(Type::Bytes), None),
        ];
        assert_eq!(
            kotlin_text("Envelope", &fields)?,
            "Envelope(name=${`name`}, value=<redacted>, parameters=${`parameters`.mapValues { entry -> if (entry.key == \"secret\") \"<redacted>\" else entry.value }}, contentBytes=${`content`.size}, saltBytes=${`salt`?.size})"
        );
        // UniFFI's Kotlin names for the fields of a tuple variant.
        assert_eq!(
            kotlin_text(
                "Raw",
                &[field("", Type::Bytes, None), field("", Type::String, None)]
            )?,
            "Raw(v1Bytes=${`v1`.size}, v2=${`v2`})"
        );
    }

    // A key reaches a string literal, so it must be a map key of plain
    // characters, whatever the metadata says.
    #[xmtp_common::test(unwrap_try = true)]
    fn key_redaction_needs_a_string_map_and_a_plain_key() {
        let fields = [field("value", Type::String, Some("@xmtp-redact=secret"))];
        let error = kotlin_text("Credential", &fields).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("Credential.value: redact = \"key\" needs a map")
        );
        assert!(swift(&groups(vec![record("Credential", fields.to_vec())])).is_err());
        for key in ["a\"b", "a\\\\b", "$x", "a}b"] {
            let fields = [field(
                "parameters",
                string_map(),
                Some(&format!("@xmtp-redact={key}")),
            )];
            let error = kotlin_text("EncodedContent", &fields).unwrap_err();
            assert!(
                error.to_string().contains("must be letters, digits"),
                "{key}: {error}"
            );
            assert!(swift(&groups(vec![record("EncodedContent", fields.to_vec())])).is_err());
        }
        let fields = [field(
            "parameters",
            string_map(),
            Some("@xmtp-redact=x-secret.v1_2"),
        )];
        assert!(kotlin_text("EncodedContent", &fields)?.contains("entry.key == \"x-secret.v1_2\""));
    }

    // Swift gets a description for each record and enum with a redacted
    // field, under UniFFI's Swift type name. An enum case never binds a
    // redacted value.
    #[xmtp_common::test(unwrap_try = true)]
    fn swift_descriptions_cover_records_and_every_enum_case() {
        let channel = enumeration(
            "NotificationChannel",
            vec![
                variant(
                    "Apns",
                    None,
                    vec![field("token", Type::String, Some("@xmtp-redact"))],
                ),
                variant(
                    "HTTPPush",
                    None,
                    vec![
                        field("url", Type::String, Some("@xmtp-redact")),
                        field("signing_key", Type::Bytes, None),
                    ],
                ),
                variant("Disabled", None, vec![]),
                // A tuple variant: UniFFI gives its fields no name.
                variant(
                    "Raw",
                    None,
                    vec![field("", Type::Bytes, None), field("", Type::String, None)],
                ),
            ],
        );
        let code = swift(&groups(vec![
            record(
                "Credential",
                vec![
                    field("name", optional(Type::String), None),
                    field("value", Type::String, Some("@xmtp-redact")),
                    field("parameters", string_map(), Some("@xmtp-redact=secret")),
                ],
            ),
            channel,
            record("Plain", vec![field("id", Type::String, None)]),
        ]))?;
        assert!(code.contains(
            "extension Credential: CustomStringConvertible, CustomDebugStringConvertible {"
        ));
        assert!(code.contains(
            "        return \"Credential(name: \\(String(reflecting: self.`name`)), value: <redacted>, parameters: \\(String(reflecting: sdkRedacted(self.`parameters`, key: \"secret\"))))\"\n"
        ));
        assert!(code.contains(
            "        case .apns: return \"NotificationChannel.apns(token: <redacted>)\"\n"
        ));
        assert!(code.contains(
            "        case let .httpPush(_, `signingKey`): return \"NotificationChannel.httpPush(url: <redacted>, signingKey: \\(String(reflecting: `signingKey`)))\"\n"
        ));
        assert!(code.contains("        case .disabled: return \"NotificationChannel.disabled\"\n"));
        assert!(code.contains(
            "        case let .raw(`v1`, `v2`): return \"NotificationChannel.raw(\\(String(reflecting: `v1`)), \\(String(reflecting: `v2`)))\"\n"
        ));
        assert!(!code.contains("``"));
        assert!(code.contains("private func sdkRedacted("));
        assert!(!code.contains("extension Plain"));
        assert!(!code.contains("`value`"));
        assert!(!code.contains("`token`"));

        // UniFFI names the Swift type `HmacKey`, not `HMACKey`.
        let code = swift(&groups(vec![record(
            "HMACKey",
            vec![field("key", Type::Bytes, Some("@xmtp-redact"))],
        )]))?;
        assert!(code.contains("extension HmacKey: CustomStringConvertible"));
        assert!(code.contains("return \"HmacKey(key: <redacted>)\""));
        assert!(!code.contains("HMACKey"));
        assert!(!code.contains("sdkRedacted"));
    }
}
