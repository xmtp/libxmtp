use std::{collections::BTreeMap, fs};

use anyhow::{Context, Result, bail};
use camino::Utf8Path;
use heck::ToLowerCamelCase;
use uniffi_meta::{Metadata, MetadataGroupMap, MethodMetadata};

use crate::Language;

/// Select only methods with the same complete Rust signature on both objects.
fn common_methods(groups: &MetadataGroupMap) -> Vec<String> {
    common_methods_from_items(groups.values().flat_map(|group| &group.items))
}

fn common_methods_from_items<'a>(items: impl IntoIterator<Item = &'a Metadata>) -> Vec<String> {
    let mut group = BTreeMap::new();
    let mut dm = BTreeMap::new();
    for metadata in items {
        let Metadata::Method(method) = metadata else {
            continue;
        };
        match method.self_name.as_str() {
            "Group" => {
                group.insert(method.name.as_str(), method);
            }
            "Dm" => {
                dm.insert(method.name.as_str(), method);
            }
            _ => {}
        }
    }
    group
        .into_iter()
        .filter_map(|(name, method)| {
            dm.get(name)
                .filter(|other| same_signature(method, other))
                .map(|_| host_name(name))
        })
        .collect()
}

fn same_signature(a: &MethodMetadata, b: &MethodMetadata) -> bool {
    a.is_async == b.is_async
        && format!("{:?}", a.return_type) == format!("{:?}", b.return_type)
        && format!("{:?}", a.throws) == format!("{:?}", b.throws)
        && a.inputs.len() == b.inputs.len()
        && a.inputs.iter().zip(&b.inputs).all(|(left, right)| {
            left.name == right.name && format!("{:?}", left.ty) == format!("{:?}", right.ty)
        })
}

fn host_name(name: &str) -> String {
    // The stock binding rename table spells an ID segment with both capitals.
    name.split('_')
        .enumerate()
        .fold(String::new(), |mut result, (index, word)| {
            if word == "id" {
                result.push_str(if index == 0 { "id" } else { "ID" });
            } else if word == "ids" {
                result.push_str(if index == 0 { "ids" } else { "IDs" });
            } else if index == 0 {
                result.push_str(word);
            } else {
                result.push_str(&word.to_lower_camel_case().to_uppercase_first());
            }
            result
        })
}

trait UppercaseFirst {
    fn to_uppercase_first(&self) -> String;
}

impl UppercaseFirst for str {
    fn to_uppercase_first(&self) -> String {
        let mut chars = self.chars();
        match chars.next() {
            Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            None => String::new(),
        }
    }
}

fn declarations(source: &str, start: &str, prefix: &str) -> Result<BTreeMap<String, String>> {
    let body = source
        .split_once(start)
        .with_context(|| format!("generated binding has no {start}"))?
        .1
        .split_once("\n}")
        .with_context(|| format!("generated binding has no end for {start}"))?
        .0;
    Ok(body
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let declaration = line.strip_prefix(prefix).or_else(|| {
                line.strip_prefix("suspend ")
                    .and_then(|rest| rest.strip_prefix(prefix))
            })?;
            let name = declaration.split_once('(')?.0.trim_matches('`');
            Some((name.to_string(), line.to_string()))
        })
        .collect())
}

fn arguments(declaration: &str, swift: bool) -> Vec<String> {
    let Some(parameters) = declaration
        .split_once('(')
        .and_then(|(_, rest)| rest.split_once(')'))
    else {
        return Vec::new();
    };
    parameters
        .0
        .split(',')
        .filter_map(|parameter| {
            parameter
                .split_once(':')
                .map(|(name, _)| name.trim().trim_matches('`'))
        })
        .map(|name| {
            if swift {
                format!("{name}: {name}")
            } else {
                name.to_string()
            }
        })
        .collect()
}

fn render(
    selected: &[String],
    group: &BTreeMap<String, String>,
    dm: &BTreeMap<String, String>,
    language: Language,
) -> Result<String> {
    let swift = matches!(language, Language::Swift);
    let mut methods = String::new();
    for name in selected {
        let (Some(g), Some(d)) = (group.get(name), dm.get(name)) else {
            bail!("{name}: common method is missing from generated bindings");
        };
        if g != d {
            // The stock generator may project the same Rust type differently.
            continue;
        }
        let args = arguments(g, swift).join(", ");
        if swift {
            let signature = g.strip_prefix("func ").expect("Swift declaration");
            let effect = if signature.contains("async throws") {
                "try await "
            } else if signature.contains(" async ") {
                "await "
            } else if signature.contains(" throws ") {
                "try "
            } else {
                ""
            };
            methods.push_str(&format!(
                "    func {signature} {{\n        switch self {{\n        case .group(let group): return {effect}group.{name}({args})\n        case .dm(let dm): return {effect}dm.{name}({args})\n        }}\n    }}\n\n"
            ));
        } else {
            let signature = g.replace("fun `", "fun Conversation.`").replace('`', "");
            methods.push_str(&format!(
                "{signature} = when (this) {{\n    is Conversation.Group -> group.{name}({args})\n    is Conversation.Dm -> dm.{name}({args})\n}}\n\n"
            ));
        }
    }
    Ok(methods)
}

pub(crate) fn generate(
    groups: &MetadataGroupMap,
    language: Language,
    out: &Utf8Path,
) -> Result<()> {
    let selected = common_methods(groups);
    let (binding, group_start, dm_start, prefix, template, filename) = match language {
        Language::Swift => (
            out.join("xmtp_sdk.swift"),
            "public protocol GroupProtocol:",
            "public protocol DmProtocol:",
            "func ",
            include_str!("../templates/swift/conversation_forwarding.swift"),
            "ConversationForwarding.swift",
        ),
        Language::Kotlin => (
            out.join("uniffi/xmtp_sdk/xmtp_sdk.kt"),
            "public interface GroupInterface {",
            "public interface DmInterface {",
            "fun ",
            include_str!("../templates/kotlin/conversation_forwarding.kt"),
            "ConversationForwarding.kt",
        ),
        _ => bail!("conversation forwarding needs Swift or Kotlin"),
    };
    let source = fs::read_to_string(&binding).with_context(|| format!("read {binding}"))?;
    let group = declarations(&source, group_start, prefix)?;
    let dm = declarations(&source, dm_start, prefix)?;
    let rendered = render(&selected, &group, &dm, language)?;
    if rendered.is_empty() {
        bail!("no common Conversation methods were generated");
    }
    fs::write(
        out.join("runtime").join(filename),
        template.replace(
            if matches!(language, Language::Swift) {
                "    // __CONVERSATION_METHODS__"
            } else {
                "private object ConversationForwardingTemplate"
            },
            &rendered,
        ),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uniffi_meta::{FnParamMetadata, Type};

    fn method(owner: &str, name: &str, ret: Option<Type>) -> MethodMetadata {
        MethodMetadata {
            module_path: "test".into(),
            self_name: owner.into(),
            name: name.into(),
            orig_name: None,
            is_async: true,
            inputs: Vec::<FnParamMetadata>::new(),
            return_type: ret,
            throws: None,
            takes_self_by_arc: true,
            checksum: None,
            docstring: None,
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn common_method_forwards_and_one_sided_method_does_not() {
        let items = vec![
            Metadata::Method(method("Group", "send_text", Some(Type::String))),
            Metadata::Method(method("Dm", "send_text", Some(Type::String))),
            Metadata::Method(method("Group", "add_members", None)),
        ];
        let selected = common_methods_from_items(&items);
        assert_eq!(selected, vec!["sendText".to_string()]);
        let group = BTreeMap::from([
            (
                "sendText".into(),
                "func sendText(text: String) async throws -> MessageID".into(),
            ),
            ("addMembers".into(), "func addMembers() async throws".into()),
        ]);
        let dm = BTreeMap::from([(
            "sendText".into(),
            "func sendText(text: String) async throws -> MessageID".into(),
        )]);
        let output = render(&selected, &group, &dm, Language::Swift).unwrap();
        assert!(output.contains("group.sendText(text: text)"));
        assert!(output.contains("dm.sendText(text: text)"));
        assert!(!output.contains("addMembers"));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn different_return_type_is_not_forwarded() {
        let group = method("Group", "state", Some(Type::String));
        let dm = method("Dm", "state", Some(Type::Bytes));
        assert!(!same_signature(&group, &dm));
    }
}
