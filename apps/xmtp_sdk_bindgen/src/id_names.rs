use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
};

use anyhow::{Context, Result, bail};
use camino::Utf8Path;
use heck::{ToLowerCamelCase, ToUpperCamelCase};
use uniffi_meta::{FieldMetadata, FnParamMetadata, Metadata, MetadataGroupMap};

/// Use the same rename table as Swift and Kotlin until the fork supports it.
pub(crate) fn typescript_rename_map(
    groups: &MetadataGroupMap,
    uniffi_config: &Utf8Path,
) -> Result<BTreeMap<String, String>> {
    let config: toml::Value = toml::from_str(
        &fs::read_to_string(uniffi_config).with_context(|| format!("read {uniffi_config}"))?,
    )?;
    let rename = |language| -> Result<&toml::value::Table> {
        config
            .get("bindings")
            .and_then(|value| value.get(language))
            .and_then(|value| value.get("rename"))
            .and_then(toml::Value::as_table)
            .with_context(|| format!("{uniffi_config}: missing {language} rename table"))
    };
    let typescript = rename("typescript")?;
    if typescript != rename("swift")? || typescript != rename("kotlin")? {
        bail!("{uniffi_config}: TypeScript ID names differ from Swift or Kotlin");
    }
    map_from_table(typescript, &metadata_paths(groups))
}

fn map_from_table(
    rename: &toml::value::Table,
    metadata_paths: &BTreeSet<String>,
) -> Result<BTreeMap<String, String>> {
    let mut names = BTreeMap::new();
    for (path, value) in rename {
        if !metadata_paths.contains(path) {
            bail!("{path}: ID rename has no exported metadata item");
        }
        let renamed = value
            .as_str()
            .with_context(|| format!("{path}: ID rename must be a string"))?;
        let source = path.rsplit('.').next().expect("nonempty rename path");
        let (from, to) = if path.contains('.') {
            (source.to_lower_camel_case(), renamed.to_lower_camel_case())
        } else {
            (source.to_upper_camel_case(), renamed.to_upper_camel_case())
        };
        if from != to
            && let Some(previous) = names.insert(from.clone(), to.clone())
            && previous != to
        {
            bail!("{path}: conflicting TypeScript rename for {from}");
        }
    }
    Ok(names)
}

fn metadata_paths(groups: &MetadataGroupMap) -> BTreeSet<String> {
    let mut paths = BTreeSet::new();
    for group in groups.values() {
        for item in &group.items {
            match item {
                Metadata::Object(object) => {
                    paths.insert(object.name.clone());
                }
                Metadata::CustomType(custom) => {
                    paths.insert(custom.name.clone());
                }
                Metadata::Record(record) => {
                    paths.insert(record.name.clone());
                    add_fields(&mut paths, &record.name, &record.fields);
                }
                Metadata::Enum(enumeration) => {
                    paths.insert(enumeration.name.clone());
                    for variant in &enumeration.variants {
                        add_fields(
                            &mut paths,
                            &format!("{}.{}", enumeration.name, variant.name),
                            &variant.fields,
                        );
                    }
                }
                Metadata::Method(method) => {
                    let path = format!("{}.{}", method.self_name, method.name);
                    paths.insert(path.clone());
                    add_params(&mut paths, &path, &method.inputs);
                }
                Metadata::TraitMethod(method) => {
                    let path = format!("{}.{}", method.trait_name, method.name);
                    paths.insert(path.clone());
                    add_params(&mut paths, &path, &method.inputs);
                }
                Metadata::Constructor(constructor) => {
                    let path = format!("{}.{}", constructor.self_name, constructor.name);
                    paths.insert(path.clone());
                    add_params(&mut paths, &path, &constructor.inputs);
                }
                Metadata::Func(function) => {
                    paths.insert(function.name.clone());
                    add_params(&mut paths, &function.name, &function.inputs);
                }
                _ => {}
            }
        }
    }
    paths
}

fn add_fields(paths: &mut BTreeSet<String>, owner: &str, fields: &[FieldMetadata]) {
    for field in fields {
        paths.insert(format!("{owner}.{}", field.name));
    }
}

fn add_params(paths: &mut BTreeSet<String>, owner: &str, params: &[FnParamMetadata]) {
    for param in params {
        paths.insert(format!("{owner}.{}", param.name));
    }
}

/// Limit edits to the fork's two generated binding files.
pub(crate) fn rewrite_generated_bindings(
    out: &Utf8Path,
    names: &BTreeMap<String, String>,
) -> Result<()> {
    for filename in ["xmtp_sdk.ts", "xmtp_sdk-ffi.ts"] {
        let path = out.join(filename);
        let source = fs::read_to_string(&path).with_context(|| format!("read {path}"))?;
        fs::write(&path, rewrite_identifiers(&source, names))?;
    }
    Ok(())
}

fn rewrite_identifiers(source: &str, names: &BTreeMap<String, String>) -> String {
    let mut out = String::with_capacity(source.len());
    rewrite_code(source, 0, names, &mut out, false);
    out
}

/// Copy code from `at` and rename identifiers. In a template interpolation,
/// stop after the `}` that closes it and return the next position.
fn rewrite_code(
    source: &str,
    mut at: usize,
    names: &BTreeMap<String, String>,
    out: &mut String,
    interpolation: bool,
) -> usize {
    let bytes = source.as_bytes();
    let mut depth = 0usize;
    while at < bytes.len() {
        let start = at;
        if bytes[at] == b'/' && bytes.get(at + 1) == Some(&b'/') {
            at = source[at..].find('\n').map_or(bytes.len(), |end| at + end);
            out.push_str(&rewrite_link_targets(&source[start..at], names));
            continue;
        } else if bytes[at] == b'/' && bytes.get(at + 1) == Some(&b'*') {
            at = source[at + 2..]
                .find("*/")
                .map_or(bytes.len(), |end| at + end + 4);
            out.push_str(&rewrite_link_targets(&source[start..at], names));
            continue;
        } else if bytes[at] == b'`' {
            at = rewrite_template(source, at, names, out);
            continue;
        } else if matches!(bytes[at], b'\'' | b'"') {
            let quote = bytes[at];
            at += 1;
            while at < bytes.len() {
                if bytes[at] == b'\\' {
                    at = (at + 2).min(bytes.len());
                } else if bytes[at] == quote {
                    at += 1;
                    break;
                } else {
                    at += 1;
                }
            }
        } else if is_identifier_start(bytes[at]) {
            at += 1;
            while at < bytes.len() && is_identifier_continue(bytes[at]) {
                at += 1;
            }
            let identifier = &source[start..at];
            out.push_str(&renamed_identifier(identifier, names));
            continue;
        } else if interpolation && bytes[at] == b'{' {
            depth += 1;
            at += 1;
        } else if interpolation && bytes[at] == b'}' {
            at += 1;
            if depth == 0 {
                out.push('}');
                return at;
            }
            depth -= 1;
        } else {
            at += source[at..].chars().next().expect("valid UTF-8").len_utf8();
        }
        out.push_str(&source[start..at]);
    }
    at
}

/// Copy template text unchanged, and rename identifiers in each `${...}`.
fn rewrite_template(
    source: &str,
    mut at: usize,
    names: &BTreeMap<String, String>,
    out: &mut String,
) -> usize {
    let bytes = source.as_bytes();
    let mut start = at;
    at += 1;
    while at < bytes.len() {
        if bytes[at] == b'\\' {
            at = (at + 2).min(bytes.len());
        } else if bytes[at] == b'`' {
            at += 1;
            break;
        } else if bytes[at] == b'$' && bytes.get(at + 1) == Some(&b'{') {
            at += 2;
            out.push_str(&source[start..at]);
            at = rewrite_code(source, at, names, out, true);
            start = at;
        } else {
            at += 1;
        }
    }
    out.push_str(&source[start..at]);
    at
}

fn rewrite_link_targets(comment: &str, names: &BTreeMap<String, String>) -> String {
    let mut out = String::with_capacity(comment.len());
    let mut remaining = comment;
    while let Some(start) = remaining.find("{@link ") {
        out.push_str(&remaining[..start]);
        remaining = &remaining[start..];
        let Some(end) = remaining.find('}') else {
            break;
        };
        let link = &remaining["{@link ".len()..end];
        out.push_str("{@link ");
        out.push_str(names.get(link).map_or(link, String::as_str));
        out.push('}');
        remaining = &remaining[end + 1..];
    }
    out.push_str(remaining);
    out
}

fn renamed_identifier<'a>(identifier: &'a str, names: &'a BTreeMap<String, String>) -> String {
    if let Some(renamed) = names.get(identifier) {
        return renamed.clone();
    }
    // UniFFI derives converter symbols from exported type names.
    for (from, to) in names {
        if from.starts_with(|character: char| character.is_ascii_uppercase())
            && let Some(prefix) = identifier.strip_suffix(from)
            && !prefix.is_empty()
        {
            return format!("{prefix}{to}");
        }
    }
    identifier.to_owned()
}

fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || matches!(byte, b'_' | b'$')
}

fn is_identifier_continue(byte: u8) -> bool {
    is_identifier_start(byte) || byte.is_ascii_digit()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[xmtp_common::test(unwrap_try = true)]
    fn renames_only_exported_identifiers_in_binding_code() {
        let rename: toml::value::Table =
            toml::from_str("'InboxID' = 'inbox_i_d'\n'Client.inbox_id' = 'inbox_i_d'\n")?;
        let paths = BTreeSet::from(["InboxID".into(), "Client.inbox_id".into()]);
        let names = map_from_table(&rename, &paths)?;
        assert_eq!(names.get("InboxId").map(String::as_str), Some("InboxID"));
        let input = "const inboxId: InboxId = shapeId; // inboxId\nconst text = 'inboxId'; /* InboxId */\nconst template = `inboxId`;\nconst FfiConverterTypeInboxId = 1;\n/** {@link InboxId} */";
        let expected = "const inboxID: InboxID = shapeId; // inboxId\nconst text = 'inboxId'; /* InboxId */\nconst template = `inboxId`;\nconst FfiConverterTypeInboxID = 1;\n/** {@link InboxID} */";
        assert_eq!(rewrite_identifiers(input, &names), expected);
        assert!(!names.contains_key("shapeId"));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn renames_identifiers_in_template_interpolations() {
        let names = BTreeMap::from([("inboxId".to_string(), "inboxID".to_string())]);
        let input = "`inboxId ${inboxId} ${f({ a: inboxId }, `${inboxId}`)} inboxId` + inboxId";
        let expected = "`inboxId ${inboxID} ${f({ a: inboxID }, `${inboxID}`)} inboxId` + inboxID";
        assert_eq!(rewrite_identifiers(input, &names), expected);
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn rename_must_match_metadata() {
        let rename: toml::value::Table = toml::from_str("'Client.inbox_id' = 'inbox_i_d'")?;
        let error = map_from_table(&rename, &BTreeSet::new()).unwrap_err();
        assert!(error.to_string().contains("Client.inbox_id"));
    }
}
