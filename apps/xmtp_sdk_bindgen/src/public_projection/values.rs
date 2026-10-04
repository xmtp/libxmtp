use std::fmt::Write as _;

use anyhow::{Result, bail};
use uniffi_meta::{EnumMetadata, RecordMetadata, Type};

use super::{camel, convert, policy::cursor_type, public_type};

pub(super) fn record(code: &mut String, record: &RecordMetadata) -> Result<()> {
    let name = &record.name;
    writeln!(code, "export type {name} = {{")?;
    for field in &record.fields {
        // A field with a Rust default can be left out; the binding factory
        // fills it in.
        let optional = if matches!(field.ty, Type::Optional { .. }) || field.default.is_some() {
            "?"
        } else {
            ""
        };
        writeln!(
            code,
            "readonly {}{optional}: {};",
            camel(&field.name),
            cursor_type(name, &camel(&field.name), public_type(&field.ty))
        )?;
    }
    code.push_str("};\n");
    for lower in [false, true] {
        let (direction, source, target) = if lower {
            ("lower", name.to_owned(), format!("B.{name}"))
        } else {
            ("lift", format!("B.{name}"), name.to_owned())
        };
        let defaults = lower && record.fields.iter().any(|field| field.default.is_some());
        let fields = record
            .fields
            .iter()
            .map(|field| {
                let field_name = camel(&field.name);
                if defaults && field.default.is_some() {
                    // An explicit undefined would replace the factory default,
                    // so a left-out field stays out.
                    let ty = match &field.ty {
                        Type::Optional { inner_type } => inner_type,
                        ty => ty,
                    };
                    let converted = convert(ty, &format!("value.{field_name}"), lower);
                    (
                        true,
                        format!(
                            "value.{field_name} === undefined ? {{}} : {{ {field_name}: {converted} }}"
                        ),
                    )
                } else {
                    let converted = convert(&field.ty, &format!("value.{field_name}"), lower);
                    (false, format!("{field_name}: {converted}"))
                }
            })
            .collect::<Vec<_>>();
        let body = match fields.as_slice() {
            [(true, only)] => only.clone(),
            fields => format!(
                "{{ {} }}",
                fields
                    .iter()
                    .map(|(spread, field)| if *spread {
                        format!("...({field})")
                    } else {
                        field.clone()
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        };
        let body = if defaults {
            format!("B.{name}.create({body})")
        } else {
            body
        };
        let guard = if lower && name == "Credential" {
            super::policy::CREDENTIAL_GUARD
        } else {
            ""
        };
        writeln!(
            code,
            "export function {direction}{name}(value: {source}, projection: ObjectProjection): {target} {{ void projection; {guard} return {body}; }}"
        )?;
    }
    Ok(())
}

pub(super) fn enumeration(code: &mut String, value: &EnumMetadata) -> Result<()> {
    let name = &value.name;
    let flat = !value.shape.is_error() && value.variants.iter().all(|v| v.fields.is_empty());
    writeln!(code, "export type {name} =")?;
    for variant in &value.variants {
        let kind = public_variant_kind(name, &variant.name)?;
        if flat {
            writeln!(code, "| '{kind}'")?;
            continue;
        }
        writeln!(code, "| {{ readonly kind: '{kind}';")?;
        for (i, field) in variant.fields.iter().enumerate() {
            let field_name = if field.name.is_empty() {
                if variant.fields.len() == 1 {
                    "value".into()
                } else {
                    format!("value{i}")
                }
            } else {
                camel(&field.name)
            };
            let optional = if matches!(field.ty, Type::Optional { .. }) {
                "?"
            } else {
                ""
            };
            writeln!(
                code,
                "readonly {field_name}{optional}: {};",
                public_type(&field.ty)
            )?;
        }
        code.push_str(super::policy::extra_variant_fields(name, &variant.name));
        code.push_str("}\n");
    }
    code.push_str(";\n");
    for lower in [false, true] {
        let (direction, source, target) = if lower {
            ("lower", name.to_owned(), format!("B.{name}"))
        } else {
            ("lift", format!("B.{name}"), name.to_owned())
        };
        let discriminant = if flat {
            "value"
        } else if lower {
            "value.kind"
        } else {
            "value.tag"
        };
        let single = value.variants.len() == 1;
        writeln!(
            code,
            "export function {direction}{name}(value: {source}, projection: ObjectProjection): {target} {{ void projection;"
        )?;
        if lower && name == "AttachmentSource" {
            code.push_str(super::policy::ATTACHMENT_SOURCE_GUARD);
        }
        if !single {
            writeln!(code, "switch ({discriminant}) {{")?;
        }
        for variant in &value.variants {
            let kind = public_variant_kind(name, &variant.name)?;
            let raw_variant = format!("B.{name}.{}", variant.name);
            let case = if lower {
                format!("'{kind}'")
            } else if flat {
                raw_variant.clone()
            } else {
                format!("B.{name}_Tags.{}", variant.name)
            };
            let prefix = if single {
                format!("checkedVariant({discriminant}, {case});")
            } else {
                format!("case {case}:")
            };
            if flat {
                writeln!(
                    code,
                    "{prefix} return {};",
                    if lower {
                        raw_variant
                    } else {
                        format!("'{kind}'")
                    }
                )?;
                continue;
            }
            let named = variant.fields.first().is_some_and(|f| !f.name.is_empty());
            let fields = variant
                .fields
                .iter()
                .enumerate()
                .map(|(i, field)| {
                    let field_name = if field.name.is_empty() {
                        if variant.fields.len() == 1 {
                            "value".into()
                        } else {
                            format!("value{i}")
                        }
                    } else {
                        camel(&field.name)
                    };
                    let raw = if lower {
                        format!("value.{field_name}")
                    } else if named {
                        format!("value.inner.{field_name}")
                    } else {
                        format!("value.inner[{i}]")
                    };
                    let converted = convert(&field.ty, &raw, lower);
                    if !lower || named {
                        format!("{field_name}: {converted}")
                    } else {
                        converted
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            let result = if lower {
                let args = if named {
                    format!("{{{fields}}}")
                } else {
                    fields
                };
                format!("{raw_variant}.new({args})")
            } else {
                format!("{{kind: '{kind}', {fields}}}")
            };
            writeln!(code, "{prefix} return {result};")?;
        }
        if !single {
            code.push_str("default: throw new TypeError('invalid public enum'); }\n");
        }
        code.push_str("}\n");
    }
    Ok(())
}

fn public_variant_kind(name: &str, variant: &str) -> Result<String> {
    if name == "DeletionCause" {
        return Ok(match variant {
            "Deleted" => "deleted",
            "DeletedLocally" => "deleted_locally",
            _ => bail!("{name}: unmapped public cause {variant}"),
        }
        .to_owned());
    }
    if name == "RejectionCause" {
        return Ok(match variant {
            "BackendMismatch" => "backend_mismatch",
            "VersionTooOld" => "version_too_old",
            _ => bail!("{name}: unmapped public cause {variant}"),
        }
        .to_owned());
    }
    if name != "EventKind" && name != "ClientEvent" {
        return Ok(camel(variant));
    }
    // The filter kind and emitted event kind use the same public string.
    let kind = match variant {
        "ConversationJoined" => "conversation.joined",
        "ConversationRemoved" => "conversation.removed",
        "ConversationMembershipChanged" => "conversation.membership_changed",
        "ConversationMetadataChanged" => "conversation.metadata_changed",
        "ConversationPaused" => "conversation.paused",
        "MessageReceived" => "message.received",
        "MessageStatusChanged" => "message.status_changed",
        "MessageDeleted" => "message.deleted",
        "MessageExpired" => "message.expired",
        "ConsentChanged" => "consent.changed",
        "HmacKeysUpdated" => "hmac_keys.updated",
        "IdentityRegistered" => "identity.registered",
        "IdentityOwnInstallationAdded" => "identity.own_installation_added",
        "IdentityOwnInstallationRevoked" => "identity.own_installation_revoked",
        "ClientRejectedByServer" => "client.rejected_by_server",
        "ClientLockoutChanged" => "client.lockout_changed",
        "ConversationForkDetected" => "conversation.fork_detected",
        "NotificationsFailed" => "notifications.failed",
        "ArchiveRestored" => "archive.restored",
        "ConnectionStateChanged" => "connection.state_changed",
        "AttachmentUploadStarted" => "attachment.upload_started",
        "AttachmentUploadCompleted" => "attachment.upload_completed",
        "AttachmentUploadFailed" => "attachment.upload_failed",
        "AttachmentDownloadStarted" => "attachment.download_started",
        "AttachmentDownloadCompleted" => "attachment.download_completed",
        "AttachmentDownloadFailed" => "attachment.download_failed",
        "AttachmentDeleted" => "attachment.deleted",
        "Lagged" => "lagged",
        _ => bail!("{name}: unmapped public event kind {variant}"),
    };
    Ok(kind.to_owned())
}

#[cfg(test)]
mod tests {
    use super::public_variant_kind;

    #[xmtp_common::test(unwrap_try = true)]
    fn event_causes_use_specified_public_names() -> anyhow::Result<()> {
        assert_eq!(public_variant_kind("DeletionCause", "Deleted")?, "deleted");
        assert_eq!(
            public_variant_kind("DeletionCause", "DeletedLocally")?,
            "deleted_locally"
        );
        assert_eq!(
            public_variant_kind("RejectionCause", "BackendMismatch")?,
            "backend_mismatch"
        );
        assert_eq!(
            public_variant_kind("RejectionCause", "VersionTooOld")?,
            "version_too_old"
        );
        Ok(())
    }
}
