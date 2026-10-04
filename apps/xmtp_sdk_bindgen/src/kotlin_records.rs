use anyhow::{Context, Result, bail};
use heck::ToLowerCamelCase;
use uniffi_meta::{EnumMetadata, FieldMetadata, Metadata, MetadataGroupMap, RecordMetadata, Type};

/// Kotlin's generated data classes compare ByteArray by reference.
pub(crate) fn rewrite(source: &str, groups: &MetadataGroupMap) -> Result<String> {
    let mut output = source.to_owned();
    for record in groups
        .values()
        .flat_map(|group| &group.items)
        .filter_map(|item| match item {
            Metadata::Record(record) => Some(record),
            _ => None,
        })
    {
        if record.name == "Credential" {
            output = credential_display(&output, record)?;
        }
        if record.name == "StorageOptions" {
            output = storage_display(&output, record)?;
        }
        if record.name == "EncodedContent" {
            output = encoded_display(&output, record)?;
        }
        if record.name == "HmacKey" {
            output = hmac_display(&output, record)?;
        }
        if record.name == "StreamBarrierTopic" {
            output = topic_display(&output, record)?;
        }
        output = attachment_display(&output, record)?;
        if !record.fields.iter().any(|field| byte_field(&field.ty)) {
            continue;
        }
        let anchor = format!("data class {} (", record.name);
        let start = output
            .find(&anchor)
            .with_context(|| format!("{}: generated Kotlin record was not found", record.name))?;
        let body = output[start..]
            .find("){\n")
            .map(|at| start + at + 3)
            .with_context(|| format!("{}: generated Kotlin record has no body", record.name))?;
        let marker = "// Generated value equality for byte fields.";
        if output[body..].starts_with(marker) {
            continue;
        }
        let code = overrides(&record.name, &record.fields);
        output.insert_str(body, &code);
    }
    for value in groups.values().flat_map(|group| &group.items) {
        if let Metadata::Enum(value) = value
            && value.name == "NotificationChannel"
        {
            output = notification_display(&output, value)?;
        }
    }
    Ok(output)
}

fn attachment_display(source: &str, record: &RecordMetadata) -> Result<String> {
    let (fields, text): (&[&str], &str) = match record.name.as_str() {
        "RemoteAttachment" => (
            &[
                "url",
                "content_digest",
                "secret",
                "salt",
                "nonce",
                "scheme",
                "content_length",
                "filename",
            ],
            "RemoteAttachment(url=<redacted>, contentDigest=$contentDigest, secret=<redacted>, salt=${salt.contentToString()}, nonce=${nonce.contentToString()}, scheme=$scheme, contentLength=$contentLength, filename=$filename)",
        ),
        "AttachmentRef" => (
            &["attachment_key", "url", "content_digest"],
            "AttachmentRef(attachmentKey=$attachmentKey, url=<redacted>, contentDigest=$contentDigest)",
        ),
        "AttachmentFailed" => (
            &["attachment_key", "url", "content_digest", "cause"],
            "AttachmentFailed(attachmentKey=$attachmentKey, url=<redacted>, contentDigest=$contentDigest, cause=$cause)",
        ),
        _ => return Ok(source.to_owned()),
    };
    if record
        .fields
        .iter()
        .map(|field| field.name.as_str())
        .ne(fields.iter().copied())
        || !record
            .fields
            .iter()
            .any(|field| field.name == "url" && field.ty == Type::String)
        || (record.name == "RemoteAttachment"
            && !record
                .fields
                .iter()
                .any(|field| field.name == "secret" && field.ty == Type::Bytes))
    {
        bail!("{}: expected the attachment fields and URL", record.name);
    }
    let anchor = format!("data class {} (", record.name);
    if source.matches(&anchor).count() != 1 {
        bail!("{}: expected one generated data class", record.name);
    }
    let start = source.find(&anchor).expect("one admitted record");
    let body = source[start..]
        .find("){\n")
        .map(|at| start + at + 3)
        .with_context(|| format!("{}: generated record has no body", record.name))?;
    let end = source[body..]
        .find("\n}")
        .map(|at| body + at)
        .with_context(|| format!("{}: generated record has no end", record.name))?;
    let display = format!("    override fun toString(): String = \"{text}\"\n\n");
    if source[body..end].contains(&display) {
        return Ok(source.to_owned());
    }
    if source[body..end].contains("fun toString(") {
        bail!("{}: generated display already exists", record.name);
    }
    let mut output = source.to_owned();
    output.insert_str(body, &display);
    Ok(output)
}

fn notification_display(source: &str, value: &EnumMetadata) -> Result<String> {
    let expected = [
        ("Apns", vec![("token", Type::String)]),
        ("Fcm", vec![("token", Type::String)]),
        (
            "Http",
            vec![("url", Type::String), ("signing_key", Type::Bytes)],
        ),
    ];
    if value.variants.len() != expected.len()
        || value
            .variants
            .iter()
            .zip(&expected)
            .any(|(variant, (name, fields))| {
                variant.name != *name
                    || variant.fields.len() != fields.len()
                    || variant
                        .fields
                        .iter()
                        .zip(fields)
                        .any(|(field, (name, ty))| field.name != *name || field.ty != *ty)
            })
    {
        bail!("NotificationChannel: expected APNS, FCM and HTTP credential fields");
    }
    let anchor = "sealed class NotificationChannel {";
    if source.matches(anchor).count() != 1 {
        bail!("NotificationChannel: expected one generated enum");
    }
    let start = source.find(anchor).expect("one admitted enum");
    let end = source[start..]
        .find("\n}\n")
        .map(|at| start + at)
        .context("NotificationChannel: generated enum has no end")?;
    let mut block = source[start..end].to_owned();
    for (name, text) in [
        ("Apns", "Apns(token=<redacted>)"),
        ("Fcm", "Fcm(token=<redacted>)"),
        ("Http", "Http(url=<redacted>, signingKey=<redacted>)"),
    ] {
        let anchor = format!("data class {name}(");
        if block.matches(&anchor).count() != 1 {
            bail!("NotificationChannel: expected one {name} variant");
        }
        let start = block.find(&anchor).expect("one admitted variant");
        let body = block[start..]
            .find("\n    {")
            .map(|at| start + at + "\n    {".len())
            .context("NotificationChannel: generated variant has no body")?;
        let end = block[body..]
            .find("companion object")
            .map(|at| body + at)
            .context("NotificationChannel: generated variant has no companion")?;
        let display = format!("\n        override fun toString(): String = \"{text}\"\n");
        if block[body..end].contains(&display) {
            continue;
        }
        if block[body..end].contains("fun toString(") {
            bail!("NotificationChannel: generated variant display already exists");
        }
        block.insert_str(body, &display);
    }
    Ok(format!("{}{block}{}", &source[..start], &source[end..]))
}

const CREDENTIAL_DISPLAY: &str = "    // Keep credential values out of diagnostic text.\n    override fun toString(): String = \"Credential(name=$name, value=<redacted>, expiresAtSeconds=$expiresAtSeconds)\"\n\n";

fn credential_display(source: &str, record: &RecordMetadata) -> Result<String> {
    if !matches!(record.fields.as_slice(), [name, value, expires]
        if name.name == "name" && matches!(&name.ty, Type::Optional { inner_type } if matches!(inner_type.as_ref(), Type::String))
            && value.name == "value" && value.ty == Type::String
            && expires.name == "expires_at_seconds" && expires.ty == Type::Int64)
    {
        bail!("Credential: expected name, value and signed expiry fields");
    }
    let anchor = "data class Credential (";
    if source.matches(anchor).count() != 1 {
        bail!("Credential: expected one generated data class");
    }
    let start = source.find(anchor).expect("one admitted record");
    let body = source[start..]
        .find("){\n")
        .map(|at| start + at + 3)
        .context("Credential: generated record has no body")?;
    if source[body..].starts_with(CREDENTIAL_DISPLAY) {
        return Ok(source.to_owned());
    }
    let end = source[body..]
        .find("\n}")
        .map(|at| body + at)
        .context("Credential: generated record has no end")?;
    if source[body..end].contains("fun toString(") {
        bail!("Credential: generated display already exists");
    }
    let mut output = source.to_owned();
    output.insert_str(body, CREDENTIAL_DISPLAY);
    Ok(output)
}

const TOPIC_DISPLAY: &str = "    // Keep full topic identifiers out of diagnostic text.\n    override fun toString(): String = \"StreamBarrierTopic(topic=<redacted>, scopeGeneration=$scopeGeneration, target=$target, received=$received, processed=$processed, unresolvedWelcomes=$unresolvedWelcomes, inactive=$inactive, cause=$cause)\"\n\n";

fn topic_display(source: &str, record: &RecordMetadata) -> Result<String> {
    let expected = [
        "topic",
        "scope_generation",
        "target",
        "received",
        "processed",
        "unresolved_welcomes",
        "inactive",
        "cause",
    ];
    if record
        .fields
        .iter()
        .map(|field| field.name.as_str())
        .ne(expected)
        || record.fields[0].ty != Type::Bytes
    {
        bail!("StreamBarrierTopic: expected the complete barrier fields and topic bytes");
    }
    let anchor = "data class StreamBarrierTopic (";
    if source.matches(anchor).count() != 1 {
        bail!("StreamBarrierTopic: expected one generated data class");
    }
    let start = source.find(anchor).expect("one admitted record");
    let body = source[start..]
        .find("){\n")
        .map(|at| start + at + 3)
        .context("StreamBarrierTopic: generated record has no body")?;
    let end = source[body..]
        .find("\n}")
        .map(|at| body + at)
        .context("StreamBarrierTopic: generated record has no end")?;
    if source[body..end].contains(TOPIC_DISPLAY) {
        return Ok(source.to_owned());
    }
    if source[body..end].contains("fun toString(") {
        bail!("StreamBarrierTopic: generated display already exists");
    }
    let mut output = source.to_owned();
    output.insert_str(body, TOPIC_DISPLAY);
    Ok(output)
}

const STORAGE_DISPLAY: &str = "    // Keep database encryption keys out of diagnostic text.\n    override fun toString(): String = \"StorageOptions(location=$location, label=$label, encryptionKey=<redacted>, pool=$pool, singleConnection=$singleConnection)\"\n\n";

fn storage_display(source: &str, record: &RecordMetadata) -> Result<String> {
    let expected = [
        "location",
        "label",
        "encryption_key",
        "pool",
        "single_connection",
    ];
    if record
        .fields
        .iter()
        .map(|field| field.name.as_str())
        .ne(expected)
        || !matches!(&record.fields[2].ty, Type::Optional { inner_type } if matches!(inner_type.as_ref(), Type::Bytes))
    {
        bail!("StorageOptions: expected the storage fields and optional encryption key bytes");
    }
    let anchor = "data class StorageOptions (";
    if source.matches(anchor).count() != 1 {
        bail!("StorageOptions: expected one generated data class");
    }
    let start = source.find(anchor).expect("one admitted record");
    let body = source[start..]
        .find("){\n")
        .map(|at| start + at + 3)
        .context("StorageOptions: generated record has no body")?;
    let end = source[body..]
        .find("\n}")
        .map(|at| body + at)
        .context("StorageOptions: generated record has no end")?;
    if source[body..end].contains(STORAGE_DISPLAY) {
        return Ok(source.to_owned());
    }
    if source[body..end].contains("fun toString(") {
        bail!("StorageOptions: generated display already exists");
    }
    let mut output = source.to_owned();
    output.insert_str(body, STORAGE_DISPLAY);
    Ok(output)
}

const HMAC_DISPLAY: &str =
    "    override fun toString(): String = \"HmacKey(key=<redacted>, epoch=$epoch)\"\n\n";

fn hmac_display(source: &str, record: &RecordMetadata) -> Result<String> {
    if record
        .fields
        .iter()
        .map(|field| field.name.as_str())
        .ne(["key", "epoch"])
        || record.fields[0].ty != Type::Bytes
        || record.fields[1].ty != Type::Int64
    {
        bail!("HmacKey: expected key bytes and a signed epoch");
    }
    let anchor = "data class HmacKey (";
    if source.matches(anchor).count() != 1 {
        bail!("HmacKey: expected one generated data class");
    }
    let start = source.find(anchor).expect("one admitted record");
    let body = source[start..]
        .find("){\n")
        .map(|at| start + at + 3)
        .context("HmacKey: generated record has no body")?;
    let end = source[body..]
        .find("\n}")
        .map(|at| body + at)
        .context("HmacKey: generated record has no end")?;
    if source[body..end].contains(HMAC_DISPLAY) {
        return Ok(source.to_owned());
    }
    if source[body..end].contains("fun toString(") {
        bail!("HmacKey: generated display already exists");
    }
    let mut output = source.to_owned();
    output.insert_str(body, HMAC_DISPLAY);
    Ok(output)
}

const ENCODED_DISPLAY: &str = r#"    override fun toString(): String = "EncodedContent(type=$type, parameters=${parameters.mapValues { (name, value) -> if (name == "secret") "<redacted>" else value }}, fallback=$fallback, content=${content.contentToString()})"

"#;

fn encoded_display(source: &str, record: &RecordMetadata) -> Result<String> {
    if record.fields.iter().map(|field| field.name.as_str()).ne([
        "type",
        "parameters",
        "fallback",
        "content",
    ]) {
        bail!("EncodedContent: expected the four envelope fields");
    }
    let anchor = "data class EncodedContent (";
    if source.matches(anchor).count() != 1 {
        bail!("EncodedContent: expected one generated data class");
    }
    let start = source.find(anchor).expect("one admitted record");
    let body = source[start..]
        .find("){\n")
        .map(|at| start + at + 3)
        .context("EncodedContent: generated record has no body")?;
    let end = source[body..]
        .find("\n}")
        .map(|at| body + at)
        .context("EncodedContent: generated record has no end")?;
    if source[body..end].contains(ENCODED_DISPLAY) {
        return Ok(source.to_owned());
    }
    if source[body..end].contains("fun toString(") {
        bail!("EncodedContent: generated display already exists");
    }
    let mut output = source.to_owned();
    output.insert_str(body, ENCODED_DISPLAY);
    Ok(output)
}

fn byte_field(ty: &Type) -> bool {
    match ty {
        Type::Bytes => true,
        Type::Optional { inner_type } => matches!(inner_type.as_ref(), Type::Bytes),
        _ => false,
    }
}

fn overrides(name: &str, fields: &[FieldMetadata]) -> String {
    let comparisons = fields
        .iter()
        .map(|field| {
            let name = field.name.to_lower_camel_case();
            if byte_field(&field.ty) {
                format!("java.util.Arrays.equals(`{name}`, other.`{name}`)")
            } else {
                format!("`{name}` == other.`{name}`")
            }
        })
        .collect::<Vec<_>>()
        .join(" &&\n            ");
    let mut code = format!(
        "    // Generated value equality for byte fields.\n    override fun equals(other: Any?): Boolean =\n        other is {name} &&\n            {comparisons}\n\n    override fun hashCode(): Int {{\n"
    );
    for (index, field) in fields.iter().enumerate() {
        let name = field.name.to_lower_camel_case();
        let hash = if byte_field(&field.ty) {
            format!("java.util.Arrays.hashCode(`{name}`)")
        } else {
            format!("`{name}`.hashCode()")
        };
        if index == 0 {
            code.push_str(&format!("        var result = {hash}\n"));
        } else {
            code.push_str(&format!("        result = 31 * result + {hash}\n"));
        }
    }
    code.push_str("        return result\n    }\n\n");
    code
}

#[cfg(test)]
mod tests {
    use super::*;

    #[xmtp_common::test(unwrap_try = true)]
    fn byte_record_uses_value_equality() {
        let fields = [
            FieldMetadata {
                name: "id".into(),
                orig_name: None,
                ty: Type::String,
                default: None,
                docstring: None,
            },
            FieldMetadata {
                name: "content".into(),
                orig_name: None,
                ty: Type::Bytes,
                default: None,
                docstring: None,
            },
        ];
        let code = overrides("Payload", &fields);
        assert!(code.contains("other is Payload"));
        assert!(code.contains("java.util.Arrays.equals(`content`, other.`content`)"));
        assert!(code.contains("java.util.Arrays.hashCode(`content`)"));
        assert!(code.contains("`id` == other.`id`"));
    }
    #[xmtp_common::test(unwrap_try = true)]
    fn credential_display_redacts_only_value_and_rejects_template_drift() {
        let field = |name: &str, ty| FieldMetadata {
            name: name.into(),
            orig_name: None,
            ty,
            default: None,
            docstring: None,
        };
        let mut record = RecordMetadata {
            module_path: "xmtp_sdk::credentials".into(),
            name: "Credential".into(),
            orig_name: None,
            remote: false,
            fields: vec![
                field(
                    "name",
                    Type::Optional {
                        inner_type: Box::new(Type::String),
                    },
                ),
                field("value", Type::String),
                field("expires_at_seconds", Type::Int64),
            ],
            docstring: None,
        };
        let source = "data class Credential (var name: String?, var value: String, var expiresAtSeconds: Long){\n\n}\n";
        let rewritten = credential_display(source, &record)?;
        assert!(rewritten.contains("name=$name"));
        assert!(rewritten.contains("expiresAtSeconds=$expiresAtSeconds"));
        assert!(rewritten.contains("value=<redacted>"));
        assert!(!rewritten.contains("$value"));
        assert!(rewritten.contains("var value: String"));
        assert_eq!(credential_display(&rewritten, &record)?, rewritten);
        assert!(credential_display(&source.replace("data class", "class"), &record).is_err());
        record.fields[2].ty = Type::UInt64;
        assert!(credential_display(source, &record).is_err());
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn topic_record_diagnostics_hide_bytes_and_keep_structured_fields() {
        let names = [
            "topic",
            "scope_generation",
            "target",
            "received",
            "processed",
            "unresolved_welcomes",
            "inactive",
            "cause",
        ];
        let record = RecordMetadata {
            module_path: "xmtp_sdk::error".into(),
            name: "StreamBarrierTopic".into(),
            orig_name: None,
            remote: false,
            fields: names
                .iter()
                .map(|name| FieldMetadata {
                    name: (*name).into(),
                    orig_name: None,
                    ty: if *name == "topic" {
                        Type::Bytes
                    } else {
                        Type::UInt64
                    },
                    default: None,
                    docstring: None,
                })
                .collect(),
            docstring: None,
        };
        let groups = MetadataGroupMap::from([(
            "xmtp_sdk".into(),
            uniffi_meta::MetadataGroup {
                namespace: uniffi_meta::NamespaceMetadata {
                    crate_name: "xmtp_sdk".into(),
                    name: "xmtp_sdk".into(),
                },
                namespace_docstring: None,
                items: std::collections::BTreeSet::from([Metadata::Record(record)]),
            },
        )]);
        let source = "data class StreamBarrierTopic (var topic: ByteArray){\n\n}\n";
        let rewritten = rewrite(source, &groups)?;
        assert!(
            rewritten.contains("topic=<redacted>"),
            "record diagnostics need a redacted topic"
        );
        assert!(!rewritten.contains("topic=$topic"));
        assert!(rewritten.contains("var topic: ByteArray"));
        assert!(rewritten.contains("scopeGeneration=$scopeGeneration"));
        assert!(rewritten.contains("processed=$processed"));
        assert!(rewritten.contains("java.util.Arrays.equals"));
        assert!(rewrite(&source.replace("data class", "class"), &groups).is_err());
    }
}
