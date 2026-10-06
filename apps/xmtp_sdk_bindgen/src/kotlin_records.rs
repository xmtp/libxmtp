use anyhow::{Context, Result, bail};
use heck::{ToLowerCamelCase, ToUpperCamelCase};
use uniffi_meta::{EnumMetadata, FieldMetadata, Metadata, MetadataGroupMap, RecordMetadata, Type};

use crate::redaction;

/// Kotlin's generated data classes compare ByteArray by reference and print
/// every field. Each record with byte fields gets value equality; each record
/// or variant with a redacted field gets a `toString` from its metadata.
pub(crate) fn rewrite(source: &str, groups: &MetadataGroupMap) -> Result<String> {
    let mut output = source.to_owned();
    for item in groups.values().flat_map(|group| &group.items) {
        match item {
            Metadata::Record(record) => {
                if redaction::redacted(&record.fields) {
                    output = record_display(&output, record)?;
                }
                if !record.fields.iter().any(|field| byte_field(&field.ty)) {
                    continue;
                }
                let class = record.name.to_upper_camel_case();
                let body = class_body(&output, &format!("data class {class} ("), &class)?;
                // The inserted block starts with its indent; compare the whole line.
                let marker = "    // Generated value equality for byte fields.";
                if output[body..].starts_with(marker) {
                    continue;
                }
                let code = overrides(&class, &record.fields);
                output.insert_str(body, &code);
            }
            Metadata::Enum(value)
                if value
                    .variants
                    .iter()
                    .any(|variant| redaction::redacted(&variant.fields)) =>
            {
                output = enum_display(&output, value)?;
            }
            _ => {}
        }
    }
    Ok(output)
}

/// Skip a Kotlin string literal that starts at `at`; returns the offset after
/// its closing quote.
fn after_string(text: &str, at: usize) -> Option<usize> {
    let mut escaped = false;
    for (offset, c) in text[at + 1..].char_indices() {
        match c {
            _ if escaped => escaped = false,
            '\\' => escaped = true,
            '"' => return Some(at + 1 + offset + 1),
            _ => {}
        }
    }
    None
}

/// The offset just inside the body of the class whose declaration starts
/// with `anchor`, an opening `(` included. The primary constructor ends at
/// its matching `)`, whatever its default values and field documentation
/// hold. UniFFI may write supertypes before the body (`): Disposable{`), but
/// nothing else.
fn class_body(source: &str, anchor: &str, class: &str) -> Result<usize> {
    if source.matches(anchor).count() != 1 {
        bail!("{class}: expected one generated Kotlin class");
    }
    let open = source.find(anchor).context("anchor")? + anchor.len();
    let mut depth = 1;
    let mut at = open;
    let close = loop {
        let rest = &source[at..];
        let Some(c) = rest.chars().next() else {
            bail!("{class}: generated Kotlin constructor does not end");
        };
        // Field KDoc is prose: its parentheses and quotes do not count.
        if rest.starts_with("/*") {
            at += rest
                .find("*/")
                .with_context(|| format!("{class}: generated Kotlin comment does not end"))?
                + 2;
            continue;
        }
        if rest.starts_with("//") {
            at += rest.find('\n').unwrap_or(rest.len());
            continue;
        }
        match c {
            '"' => {
                at = after_string(source, at)
                    .with_context(|| format!("{class}: generated Kotlin string does not end"))?;
                continue;
            }
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            break at;
        }
        at += c.len_utf8();
    };
    let rest = &source[close + 1..];
    let brace = rest
        .find('{')
        .with_context(|| format!("{class}: generated Kotlin class has no body"))?;
    let supertypes = rest[..brace].trim();
    if !(supertypes.is_empty()
        || supertypes.starts_with(':') && !supertypes.contains([';', '=', '}']))
    {
        bail!("{class}: generated Kotlin class body does not follow its constructor");
    }
    let body = close + 1 + brace + 1;
    Ok(body + usize::from(source[body..].starts_with('\n')))
}

const DISPLAY_COMMENT: &str = "    // Generated: redacted fields stay out of diagnostic text.\n";

fn record_display(source: &str, record: &RecordMetadata) -> Result<String> {
    // UniFFI's Kotlin class name.
    let class = record.name.to_upper_camel_case();
    let display = format!(
        "{DISPLAY_COMMENT}    override fun toString(): String = \"{}\"\n\n",
        redaction::kotlin_text(&class, &record.fields)?
    );
    let body = class_body(source, &format!("data class {class} ("), &class)?;
    let end = source[body..]
        .find("\n}")
        .map(|at| body + at)
        .with_context(|| format!("{class}: generated record has no end"))?;
    if source[body..end].contains(&display) {
        return Ok(source.to_owned());
    }
    if source[body..end].contains("fun toString(") {
        bail!("{class}: generated display already exists");
    }
    let mut output = source.to_owned();
    output.insert_str(body, &display);
    Ok(output)
}

fn enum_display(source: &str, value: &EnumMetadata) -> Result<String> {
    let class = value.name.to_upper_camel_case();
    let anchor = format!("sealed class {class}");
    let start = source
        .match_indices(&anchor)
        .map(|(at, _)| at)
        .filter(|at| matches!(source[at + anchor.len()..].chars().next(), Some(' ' | ':')))
        .collect::<Vec<_>>();
    let [start] = start[..] else {
        bail!("{class}: expected one generated enum");
    };
    let end = source[start..]
        .find("\n}\n")
        .map(|at| start + at)
        .with_context(|| format!("{class}: generated enum has no end"))?;
    let mut block = source[start..end].to_owned();
    for variant in value
        .variants
        .iter()
        .filter(|variant| redaction::redacted(&variant.fields))
    {
        let name = variant.name.to_upper_camel_case();
        let text = redaction::kotlin_text(&name, &variant.fields)?;
        let body = class_body(
            &block,
            &format!("data class {name}("),
            &format!("{class}.{name}"),
        )?;
        let end = block[body..]
            .find("companion object")
            .map(|at| body + at)
            .with_context(|| format!("{class}.{name}: generated variant has no companion"))?;
        let display = format!("        override fun toString(): String = \"{text}\"\n");
        if block[body..end].contains(&display) {
            continue;
        }
        if block[body..end].contains("fun toString(") {
            bail!("{class}.{name}: generated variant display already exists");
        }
        block.insert_str(body, &display);
    }
    Ok(format!("{}{block}{}", &source[..start], &source[end..]))
}

pub(crate) fn byte_field(ty: &Type) -> bool {
    match ty {
        Type::Bytes => true,
        Type::Optional { inner_type } => matches!(inner_type.as_ref(), Type::Bytes),
        _ => false,
    }
}

fn overrides(class: &str, fields: &[FieldMetadata]) -> String {
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
        "    // Generated value equality for byte fields.\n    override fun equals(other: Any?): Boolean =\n        other is {class} &&\n            {comparisons}\n\n    override fun hashCode(): Int {{\n"
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
    use crate::test_metadata::{enumeration, field, groups, record, variant};

    #[xmtp_common::test(unwrap_try = true)]
    fn byte_record_uses_value_equality() {
        let fields = [
            field("id", Type::String, None),
            field("content", Type::Bytes, None),
        ];
        let code = overrides("Payload", &fields);
        assert!(code.contains("other is Payload"));
        assert!(code.contains("java.util.Arrays.equals(`content`, other.`content`)"));
        assert!(code.contains("java.util.Arrays.hashCode(`content`)"));
        assert!(code.contains("`id` == other.`id`"));
    }

    // Any record with a redacted field gets its display from metadata: a new
    // record or a new field needs no generator change.
    #[xmtp_common::test(unwrap_try = true)]
    fn redacted_record_display_comes_from_metadata_and_is_idempotent() {
        let record = record(
            "BrandNewSecret",
            vec![
                field("label", Type::String, None),
                field("tags", crate::test_metadata::sequence(Type::String), None),
                field("value", Type::String, Some("The secret. @xmtp-redact")),
                field("key", Type::Bytes, None),
            ],
        );
        let source = "data class BrandNewSecret (\n    var `label`: kotlin.String = \"a(b\", \n    var `tags`: List<kotlin.String> = listOf(), \n    var `value`: kotlin.String, \n    var `key`: kotlin.ByteArray\n){\n\n    companion object\n}\n";
        let groups = groups(vec![record]);
        let rewritten = rewrite(source, &groups)?;
        assert!(rewritten.contains(
            "override fun toString(): String = \"BrandNewSecret(label=${`label`}, tags=${`tags`}, value=<redacted>, keyBytes=${`key`.size})\""
        ));
        assert!(!rewritten.contains("$value"));
        assert!(rewritten.contains("var `value`: kotlin.String"));
        // Byte equality still comes first in the body.
        assert!(
            rewritten.find("Generated value equality").unwrap()
                < rewritten.find("fun toString").unwrap()
        );
        assert_eq!(rewrite(&rewritten, &groups)?, rewritten);
        assert!(rewrite(&source.replace("data class", "class"), &groups).is_err());
    }

    // A record that holds an object implements Disposable; its body follows
    // the supertype. The anchor is the record's own constructor, never a
    // later `){` in the file.
    #[xmtp_common::test(unwrap_try = true)]
    fn record_body_follows_its_own_constructor_and_supertypes() {
        let groups = groups(vec![record(
            "Session",
            vec![
                field("token", Type::String, Some("@xmtp-redact")),
                field(
                    "client",
                    Type::Object {
                        module_path: "xmtp_sdk".into(),
                        name: "Client".into(),
                        imp: uniffi_meta::ObjectImpl::Struct,
                    },
                    None,
                ),
            ],
        )]);
        let source = "data class Session (\n    var `token`: kotlin.String, \n    var `client`: Client\n): Disposable{\n    \n    override fun destroy() {\n    }\n    companion object\n}\n\nfun other(){\n}\n";
        let rewritten = rewrite(source, &groups)?;
        assert!(rewritten.starts_with(
            "data class Session (\n    var `token`: kotlin.String, \n    var `client`: Client\n): Disposable{\n    // Generated: redacted fields stay out of diagnostic text.\n    override fun toString(): String = \"Session(token=<redacted>, client=${`client`})\"\n"
        ));
        assert!(rewritten.ends_with("fun other(){\n}\n"));
        // Anything but supertypes between the constructor and a brace is not
        // the record body.
        let error = rewrite(
            "data class Session (\n    var `token`: kotlin.String\n)\nfun other(){\n}\n",
            &groups,
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("Session: generated Kotlin class body does not follow its constructor"),
            "{error}"
        );
    }

    // Field documentation sits between the constructor's parentheses. Its
    // parentheses and quotes are prose, not code.
    #[xmtp_common::test(unwrap_try = true)]
    fn field_documentation_does_not_end_the_constructor() {
        let groups = groups(vec![record(
            "Blob",
            vec![
                field("content", Type::Bytes, Some("The bytes (raw.")),
                field("secret", Type::String, Some("@xmtp-redact")),
            ],
        )]);
        for doc in [
            "    /**\n     * The bytes (raw.\n     */\n",
            "    /**\n     * Say \"hi.\n     */\n",
            "    // A note ) with \" in it\n",
        ] {
            let source = format!(
                "data class Blob (\n{doc}    var `content`: kotlin.ByteArray, \n    var `secret`: kotlin.String\n){{\n    companion object\n}}\n"
            );
            let rewritten = rewrite(&source, &groups)?;
            assert!(
                rewritten.contains(
                    "var `secret`: kotlin.String\n){\n    // Generated value equality for byte fields."
                ),
                "{doc}"
            );
            assert!(rewritten.contains(
                "override fun toString(): String = \"Blob(contentBytes=${`content`.size}, secret=<redacted>)\""
            ));
        }
    }

    // UniFFI's Kotlin class name is the upper camel case of the Rust name.
    #[xmtp_common::test(unwrap_try = true)]
    fn class_names_follow_uniffi_casing() {
        let groups = groups(vec![record(
            "HMACKey",
            vec![
                field("key", Type::Bytes, Some("@xmtp-redact")),
                field("epoch", Type::Int64, None),
            ],
        )]);
        let source = "data class HmacKey (\n    var `key`: kotlin.ByteArray, \n    var `epoch`: kotlin.Long\n){\n    companion object\n}\n";
        let rewritten = rewrite(source, &groups)?;
        assert!(rewritten.contains("other is HmacKey &&"));
        assert!(rewritten.contains("\"HmacKey(key=<redacted>, epoch=${`epoch`})\""));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn unredacted_record_keeps_the_stock_display() {
        let groups = groups(vec![record("Plain", vec![field("id", Type::String, None)])]);
        let source = "data class Plain (var id: String){\n\n}\n";
        assert_eq!(rewrite(source, &groups)?, source);
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn redacted_variant_display_covers_only_marked_variants() {
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
                        field("signing_key", Type::Bytes, Some("@xmtp-redact")),
                    ],
                ),
                variant("Plain", None, vec![field("name", Type::String, None)]),
                // A tuple variant cannot hold a redacted field; it keeps the
                // stock display.
                variant("Raw", None, vec![field("", Type::Bytes, None)]),
            ],
        );
        let source = "sealed class NotificationChannel {\n    data class Apns(\n        val `token`: kotlin.String) : NotificationChannel()\n    {\n        companion object\n    }\n    data class HttpPush(\n        val `url`: kotlin.String, \n        val `signingKey`: kotlin.ByteArray) : NotificationChannel()\n    {\n        companion object\n    }\n    data class Plain(\n        val `name`: kotlin.String) : NotificationChannel()\n    {\n        companion object\n    }\n    data class Raw(\n        val v1: kotlin.ByteArray) : NotificationChannel()\n    {\n        companion object\n    }\n}\n";
        let groups = groups(vec![channel]);
        let rewritten = rewrite(source, &groups)?;
        assert!(rewritten.contains("override fun toString(): String = \"Apns(token=<redacted>)\""));
        assert!(rewritten.contains(
            "    {\n        override fun toString(): String = \"HttpPush(url=<redacted>, signingKey=<redacted>)\"\n        companion object"
        ));
        assert_eq!(rewritten.matches("fun toString").count(), 2);
        assert_eq!(rewrite(&rewritten, &groups)?, rewritten);
        assert!(rewrite(&source.replace("sealed class", "class"), &groups).is_err());
        // An enum that holds an object is Disposable.
        let disposable = source.replace(
            "sealed class NotificationChannel {",
            "sealed class NotificationChannel: Disposable  {",
        );
        assert_eq!(
            rewrite(&disposable, &groups)?
                .matches("fun toString")
                .count(),
            2
        );
    }
}
