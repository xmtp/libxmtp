//! Turning one `GetConfigurationResponse` into the snapshot the client holds.
//!
//! Zero or empty on the wire means "not provided", so every such field falls
//! back to the compiled `BACKEND_DEFAULT_*` constant. The two
//! exceptions are the identifier, which is rejected when
//! empty, and `commit_log_enabled`, which keeps `None` distinct from
//! `Some(false)`.
//!
//! The application catalogue converts both ways, so the backend publishes
//! and a client registers the same definitions the snapshot holds.

use xmtp_configuration::{
    ApplicationComponentDefinition, AttachmentsConfiguration, AuthConfiguration,
    BACKEND_DEFAULT_MAX_UPLOAD_BYTES, ComponentPermissions, LimitsConfiguration, MetadataPolicy,
    MlsConfiguration, RetentionConfiguration, ServerConfiguration, SigningKeyDescription,
    attachments::{
        AttachmentConfigurationError, check_base_url, check_max_upload_bytes,
        check_retention_seconds,
    },
};

use crate::backend_v1;
use crate::xmtp::mls::message_contents::{
    self as mls,
    metadata_policy::{AndCondition, AnyCondition, Kind},
};

/// Take the published value, or the compiled default when it is zero.
// implements: CONF-025
fn or_default<T, W>(published: W, default: T) -> T
where
    T: Copy + PartialEq + Default + TryFrom<W>,
    W: Copy,
{
    T::try_from(published)
        .ok()
        .filter(|value| *value != T::default())
        .unwrap_or(default)
}

impl From<backend_v1::AuthConfiguration> for AuthConfiguration {
    fn from(auth: backend_v1::AuthConfiguration) -> Self {
        Self {
            enabled: auth.enabled,
            keys: auth
                .keys
                .into_iter()
                .map(|key| SigningKeyDescription {
                    kid: key.kid,
                    alg: key.alg,
                })
                .collect(),
            audiences: auth.audiences,
            issuers: auth.issuers,
            required_scopes: auth.required_scopes,
        }
    }
}

impl From<backend_v1::RetentionConfiguration> for RetentionConfiguration {
    fn from(retention: backend_v1::RetentionConfiguration) -> Self {
        let default = Self::default();
        Self {
            group_message_seconds: or_default(
                retention.group_message_seconds,
                default.group_message_seconds,
            ),
            welcome_seconds: or_default(retention.welcome_seconds, default.welcome_seconds),
            key_package_seconds: or_default(
                retention.key_package_seconds,
                default.key_package_seconds,
            ),
        }
    }
}

impl From<backend_v1::LimitsConfiguration> for LimitsConfiguration {
    fn from(limits: backend_v1::LimitsConfiguration) -> Self {
        let default = Self::default();
        Self {
            max_envelope_bytes: or_default(limits.max_envelope_bytes, default.max_envelope_bytes),
            max_request_bytes: or_default(limits.max_request_bytes, default.max_request_bytes),
            max_response_bytes: or_default(limits.max_response_bytes, default.max_response_bytes),
            max_publish_topics: or_default(limits.max_publish_topics, default.max_publish_topics),
            max_query_topics: or_default(limits.max_query_topics, default.max_query_topics),
            max_query_limit: or_default(limits.max_query_limit, default.max_query_limit),
            default_query_limit: or_default(
                limits.default_query_limit,
                default.default_query_limit,
            ),
            max_newest_metadata_topics: or_default(
                limits.max_newest_metadata_topics,
                default.max_newest_metadata_topics,
            ),
            max_newest_full_topics: or_default(
                limits.max_newest_full_topics,
                default.max_newest_full_topics,
            ),
            max_update_adds: or_default(limits.max_update_adds, default.max_update_adds),
            max_update_removes: or_default(limits.max_update_removes, default.max_update_removes),
            max_stream_topics: or_default(limits.max_stream_topics, default.max_stream_topics),
            max_static_topics: or_default(limits.max_static_topics, default.max_static_topics),
            max_lookup_identifiers: or_default(
                limits.max_lookup_identifiers,
                default.max_lookup_identifiers,
            ),
            max_scw_signatures: or_default(limits.max_scw_signatures, default.max_scw_signatures),
            max_identity_entries: or_default(
                limits.max_identity_entries,
                default.max_identity_entries,
            ),
            max_update_frames_per_second: or_default(
                limits.max_update_frames_per_second,
                default.max_update_frames_per_second,
            ),
            max_update_burst: or_default(limits.max_update_burst, default.max_update_burst),
            max_ping_frames_per_second: or_default(
                limits.max_ping_frames_per_second,
                default.max_ping_frames_per_second,
            ),
            max_ping_burst: or_default(limits.max_ping_burst, default.max_ping_burst),
        }
    }
}

impl From<backend_v1::MlsConfiguration> for MlsConfiguration {
    fn from(mls: backend_v1::MlsConfiguration) -> Self {
        let default = Self::default();
        Self {
            max_group_members: or_default(mls.max_group_members, default.max_group_members),
            max_installations_per_inbox: or_default(
                mls.max_installations_per_inbox,
                default.max_installations_per_inbox,
            ),
            // Absent is not the same as false: an operator may switch the
            // commit log off explicitly.
            commit_log_enabled: mls.commit_log_enabled,
        }
    }
}

impl From<mls::MetadataPolicy> for MetadataPolicy {
    fn from(policy: mls::MetadataPolicy) -> Self {
        let all =
            |policies: Vec<mls::MetadataPolicy>| policies.into_iter().map(Self::from).collect();
        match policy.kind {
            Some(Kind::Base(base)) => Self::Base(base),
            Some(Kind::AndCondition(and)) => Self::And(all(and.policies)),
            Some(Kind::AnyCondition(any)) => Self::Any(all(any.policies)),
            // A policy with no kind evaluates as the unspecified base.
            None => Self::Base(0),
        }
    }
}

impl From<MetadataPolicy> for mls::MetadataPolicy {
    fn from(policy: MetadataPolicy) -> Self {
        let all = |policies: Vec<MetadataPolicy>| policies.into_iter().map(Self::from).collect();
        let kind = match policy {
            MetadataPolicy::Base(base) => Kind::Base(base),
            MetadataPolicy::And(policies) => Kind::AndCondition(AndCondition {
                policies: all(policies),
            }),
            MetadataPolicy::Any(policies) => Kind::AnyCondition(AnyCondition {
                policies: all(policies),
            }),
        };
        Self { kind: Some(kind) }
    }
}

impl From<mls::ComponentPermissions> for ComponentPermissions {
    fn from(permissions: mls::ComponentPermissions) -> Self {
        Self {
            insert: permissions.insert_policy.map(Into::into),
            update: permissions.update_policy.map(Into::into),
            delete: permissions.delete_policy.map(Into::into),
        }
    }
}

impl From<ComponentPermissions> for mls::ComponentPermissions {
    fn from(permissions: ComponentPermissions) -> Self {
        Self {
            insert_policy: permissions.insert.map(Into::into),
            update_policy: permissions.update.map(Into::into),
            delete_policy: permissions.delete.map(Into::into),
        }
    }
}

impl From<backend_v1::ApplicationComponentDefinition> for ApplicationComponentDefinition {
    fn from(definition: backend_v1::ApplicationComponentDefinition) -> Self {
        Self {
            // Saturating keeps an oversized ID outside the application range,
            // so validation refuses it rather than reading a truncated ID.
            component_id: u16::try_from(definition.component_id).unwrap_or(u16::MAX),
            name: definition.name,
            component_type: definition.component_type,
            permissions: definition.permissions.unwrap_or_default().into(),
            in_groups: definition.in_groups,
            in_dms: definition.in_dms,
        }
    }
}

impl From<ApplicationComponentDefinition> for backend_v1::ApplicationComponentDefinition {
    fn from(definition: ApplicationComponentDefinition) -> Self {
        Self {
            component_id: definition.component_id.into(),
            name: definition.name,
            component_type: definition.component_type,
            permissions: Some(definition.permissions.into()),
            in_groups: definition.in_groups,
            in_dms: definition.in_dms,
        }
    }
}

// implements: ATCH-008, CONF-025
impl TryFrom<backend_v1::AttachmentsConfiguration> for AttachmentsConfiguration {
    type Error = AttachmentConfigurationError;

    fn try_from(attachments: backend_v1::AttachmentsConfiguration) -> Result<Self, Self::Error> {
        let max_upload_bytes = or_default(
            attachments.max_upload_bytes,
            BACKEND_DEFAULT_MAX_UPLOAD_BYTES,
        );
        check_base_url(&attachments.base_url)?;
        check_max_upload_bytes(max_upload_bytes)?;
        check_retention_seconds(attachments.retention_seconds)?;
        Ok(Self {
            base_url: attachments.base_url,
            max_upload_bytes,
            retention_seconds: attachments.retention_seconds,
        })
    }
}

impl From<backend_v1::GetConfigurationResponse> for ServerConfiguration {
    fn from(response: backend_v1::GetConfigurationResponse) -> Self {
        let attachments = response.attachments.and_then(|message| {
            match AttachmentsConfiguration::try_from(message) {
                Ok(attachments) => Some(attachments),
                Err(reason) => {
                    tracing::warn!(
                        field = reason.field(),
                        reason = reason.reason(),
                        "ignoring unusable attachment storage offer"
                    );
                    None
                }
            }
        });
        Self {
            identifier: response.identifier,
            server_version: response.server_version,
            min_libxmtp_version: response.min_libxmtp_version,
            auth: response.auth.unwrap_or_default().into(),
            retention: response.retention.unwrap_or_default().into(),
            limits: response.limits.unwrap_or_default().into(),
            mls: response.mls.unwrap_or_default().into(),
            attachments,
            smart_contract_wallet_chains: response.smart_contract_wallet_chains,
            application_components: response
                .application_components
                .into_iter()
                .map(Into::into)
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests;
