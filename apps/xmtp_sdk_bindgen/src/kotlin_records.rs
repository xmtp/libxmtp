use anyhow::{Context, Result};
use heck::ToLowerCamelCase;
use uniffi_meta::{FieldMetadata, Metadata, MetadataGroupMap, Type};

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
}
