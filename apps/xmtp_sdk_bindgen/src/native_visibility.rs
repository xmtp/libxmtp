use anyhow::{Context, Result, bail};
use heck::ToLowerCamelCase;
use uniffi_meta::{Metadata, MetadataGroupMap};

use crate::markers;

const NAME: &str = "sdkDiscardUnreturnedClient";

fn replace(source: &str, declaration: &str, internal: &str) -> Result<String> {
    if !source.contains(NAME) {
        return Ok(source.to_owned());
    }
    if source.matches(declaration).count() != 1 {
        bail!("private Client discard declaration changed");
    }
    Ok(source.replacen(declaration, internal, 1))
}

pub fn swift(source: &str, groups: &MetadataGroupMap) -> Result<String> {
    let source = host_methods(source, groups, true)?;
    replace(
        &source,
        "public func sdkDiscardUnreturnedClient(",
        "internal func sdkDiscardUnreturnedClient(",
    )
}

pub fn kotlin(source: &str, groups: &MetadataGroupMap) -> Result<String> {
    let source = host_methods(source, groups, false)?;
    replace(
        &source,
        "suspend fun `sdkDiscardUnreturnedClient`(",
        "internal suspend fun `sdkDiscardUnreturnedClient`(",
    )
}

// Select declarations inside their metadata receiver. An unrelated method can
// have the same name and must keep its public visibility.
fn host_methods(source: &str, groups: &MetadataGroupMap, swift: bool) -> Result<String> {
    let mut source = source.to_owned();
    for item in groups.values().flat_map(|group| &group.items) {
        let Metadata::Method(method) = item else {
            continue;
        };
        if !markers::has(method.docstring.as_deref(), markers::HOST_INTERNAL) {
            continue;
        }
        let name = method.name.to_lower_camel_case();
        let receiver = &method.self_name;
        let (interface, concrete, declaration, implementation, internal) = if swift {
            (
                format!("public protocol {receiver}Protocol:"),
                format!("open class {receiver}:"),
                format!("func {name}("),
                format!("open func {name}("),
                format!("internal func {name}("),
            )
        } else {
            let suspend = if method.is_async { "suspend " } else { "" };
            (
                format!("public interface {receiver}Interface {{"),
                format!("open class {receiver}:"),
                format!("{suspend}fun `{name}`("),
                format!("override {suspend}fun `{name}`("),
                format!("internal open {suspend}fun `{name}`("),
            )
        };
        source = rewrite_block(&source, &interface, &declaration, None)?;
        source = rewrite_block(&source, &concrete, &implementation, Some(&internal))?;
    }
    Ok(source)
}

fn rewrite_block(
    source: &str,
    header: &str,
    declaration: &str,
    replacement: Option<&str>,
) -> Result<String> {
    if source.matches(header).count() != 1 {
        bail!("private host receiver {header}: expected one declaration");
    }
    let start = source
        .find(header)
        .context("private host receiver missing")?;
    let end = block_end(source, start)
        .with_context(|| format!("private host receiver {header}: missing end"))?;
    let body = &source[start..end];
    if body.matches(declaration).count() != 1 {
        bail!("private host receiver {header}: expected one {declaration}");
    }
    let body = if let Some(replacement) = replacement {
        body.replacen(declaration, replacement, 1)
    } else {
        let lines = body
            .lines()
            .filter(|line| !line.trim().starts_with(declaration))
            .collect::<Vec<_>>()
            .join("\n");
        format!("{lines}\n")
    };
    Ok(format!("{}{}{}", &source[..start], body, &source[end..]))
}

// Generated method bodies can end at column zero. Count code braces and skip
// comments and strings instead of using indentation as a class boundary.
fn block_end(source: &str, start: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut index = start;
    let mut depth = 0usize;
    let mut opened = false;
    while index < bytes.len() {
        let rest = &bytes[index..];
        if rest.starts_with(b"//") {
            index += rest
                .iter()
                .position(|byte| *byte == b'\n')
                .unwrap_or(rest.len());
        } else if rest.starts_with(b"/*") {
            index += 2;
            let mut comments = 1;
            while index < bytes.len() && comments > 0 {
                if bytes[index..].starts_with(b"/*") {
                    comments += 1;
                    index += 2;
                } else if bytes[index..].starts_with(b"*/") {
                    comments -= 1;
                    index += 2;
                } else {
                    index += 1;
                }
            }
        } else if rest.starts_with(b"\"\"\"") {
            index += 3;
            while index < bytes.len() && !bytes[index..].starts_with(b"\"\"\"") {
                index += 1;
            }
            index += 3;
        } else if bytes[index] == b'"' || bytes[index] == b'\'' {
            let quote = bytes[index];
            index += 1;
            while index < bytes.len() {
                if bytes[index] == b'\\' {
                    index += 2;
                } else if bytes[index] == quote {
                    index += 1;
                    break;
                } else {
                    index += 1;
                }
            }
        } else {
            match bytes[index] {
                b'{' => {
                    depth += 1;
                    opened = true;
                }
                b'}' if opened => {
                    depth = depth.checked_sub(1)?;
                    if depth == 0 {
                        return Some(index);
                    }
                }
                _ => {}
            }
            index += 1;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_metadata::groups;
    use uniffi_meta::{MethodMetadata, Type};

    fn private_method(receiver: &str) -> Metadata {
        Metadata::Method(MethodMetadata {
            module_path: "xmtp_sdk".into(),
            self_name: receiver.into(),
            name: "renamed_owner".into(),
            orig_name: None,
            is_async: false,
            inputs: vec![],
            return_type: Some(Type::UInt64),
            throws: None,
            takes_self_by_arc: false,
            checksum: None,
            docstring: Some("@xmtp-host-internal @xmtp-internal @xmtp-immutable".into()),
        })
    }

    fn fixture(receiver: &str, swift: bool) -> String {
        if swift {
            format!(
                "public protocol {receiver}Protocol: AnyObject {{\nfunc renamedOwner() -> UInt64\n}}\nopen class {receiver}: {receiver}Protocol {{\nopen func renamedOwner() -> UInt64 {{ return 1 }}\n}}\n"
            )
        } else {
            format!(
                "public interface {receiver}Interface {{\nfun `renamedOwner`(): kotlin.ULong\n}}\nopen class {receiver}: {receiver}Interface {{\noverride fun `renamedOwner`(): kotlin.ULong {{ return 1u }}\n}}\n"
            )
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn renamed_private_methods_and_fourth_receiver_keep_unrelated_method_public() {
        let receivers = ["Alpha", "Beta", "Gamma", "Fourth"];
        let mut items = receivers
            .iter()
            .map(|name| private_method(name))
            .collect::<Vec<_>>();
        let mut legacy = private_method("Unrelated");
        let Metadata::Method(method) = &mut legacy else {
            unreachable!()
        };
        method.docstring = Some("@xmtp-internal @xmtp-immutable".into());
        items.push(legacy);
        let metadata = groups(items);
        for swift in [true, false] {
            let public = fixture("Unrelated", swift);
            let source = receivers
                .iter()
                .map(|name| fixture(name, swift))
                .collect::<String>()
                + &public;
            let output = host_methods(&source, &metadata, swift)?;
            assert!(output.contains(&public));
            assert_eq!(
                output
                    .matches(if swift {
                        "internal func renamedOwner("
                    } else {
                        "internal open fun `renamedOwner`("
                    })
                    .count(),
                4
            );
            assert_eq!(
                output
                    .matches(if swift {
                        "\nfunc renamedOwner("
                    } else {
                        "\nfun `renamedOwner`("
                    })
                    .count(),
                1
            );
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn private_metadata_rejects_wrong_locations_missing_exclusion_and_duplicates() {
        let mut method = private_method("Fourth");
        crate::markers::validate(&groups(vec![method.clone()]))?;
        let Metadata::Method(value) = &mut method else {
            unreachable!()
        };
        value.docstring = Some("@xmtp-host-internal".into());
        assert!(crate::markers::validate(&groups(vec![method.clone()])).is_err());
        let Metadata::Method(value) = &mut method else {
            unreachable!()
        };
        value.docstring = Some("@xmtp-host-internal @xmtp-host-internal @xmtp-internal".into());
        assert!(crate::markers::validate(&groups(vec![method])).is_err());
        let mut record = crate::test_metadata::record("Record", vec![]);
        let Metadata::Record(value) = &mut record else {
            unreachable!()
        };
        value.docstring = Some("@xmtp-host-internal @xmtp-internal".into());
        assert!(crate::markers::validate(&groups(vec![record])).is_err());
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn receiver_boundary_ignores_method_indentation_and_comment_braces() {
        let source = "open class Fourth: FourthProtocol {\n/* { /* } */ } */\nopen func other() {\nlet brace = \"}\"\n}\nopen func renamedOwner() -> UInt64 { return 1 }\n}\n";
        let end = block_end(source, 0)?;
        assert_eq!(&source[end..], "}\n");
        let output = rewrite_block(
            source,
            "open class Fourth:",
            "open func renamedOwner(",
            Some("internal func renamedOwner("),
        )?;
        assert!(output.contains("internal func renamedOwner("));
        assert!(output.contains("open func other()"));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn missing_interface_and_duplicate_implementation_stop_generation() {
        let metadata = groups(vec![private_method("Fourth")]);
        for swift in [true, false] {
            let source = fixture("Fourth", swift);
            let interface = if swift {
                "func renamedOwner() -> UInt64\n"
            } else {
                "fun `renamedOwner`(): kotlin.ULong\n"
            };
            assert!(host_methods(&source.replacen(interface, "", 1), &metadata, swift).is_err());
            let implementation = if swift {
                "open func renamedOwner("
            } else {
                "override fun `renamedOwner`("
            };
            assert!(
                host_methods(
                    &source.replace(implementation, &format!("{implementation}{implementation}")),
                    &metadata,
                    swift
                )
                .is_err()
            );
        }
    }
}
