use anyhow::{Context, Result, bail};
use heck::ToLowerCamelCase;
use uniffi_meta::{FieldMetadata, Metadata, MetadataGroupMap, RecordMetadata, Type};

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
        if record.name == "StreamBarrierTopic" {
            output = topic_display(&output, record)?;
        }
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
