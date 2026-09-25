//! Turning one `GetConfigurationResponse` into the snapshot the client holds.
//!
//! Zero or empty on the wire means "not provided", so every such field falls
//! back to the compiled `BACKEND_DEFAULT_*` constant. The two
//! exceptions are the identifier, which is rejected when
//! empty, and `commit_log_enabled`, which keeps `None` distinct from
//! `Some(false)`.

use xmtp_configuration::{
    AttachmentsConfiguration, AuthConfiguration, BACKEND_DEFAULT_MAX_UPLOAD_BYTES,
    LimitsConfiguration, MlsConfiguration, RetentionConfiguration, ServerConfiguration,
    SigningKeyDescription,
};

use crate::backend_v1;

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

// implements: ATCH-008, CONF-025
impl TryFrom<backend_v1::AttachmentsConfiguration> for AttachmentsConfiguration {
    type Error = &'static str;

    fn try_from(attachments: backend_v1::AttachmentsConfiguration) -> Result<Self, Self::Error> {
        if attachments.base_url.ends_with('/') {
            return Err("base_url has a trailing slash");
        }
        let base_url = url::Url::parse(&attachments.base_url)
            .map_err(|_| "base_url is not an absolute URL")?;
        if base_url.host().is_none() {
            return Err("base_url has no host");
        }
        let accepted_scheme = match base_url.scheme() {
            "https" => true,
            "http" => match base_url.host() {
                Some(url::Host::Domain(host)) => host == "localhost",
                Some(url::Host::Ipv4(address)) => address.is_loopback(),
                Some(url::Host::Ipv6(address)) => address.is_loopback(),
                None => false,
            },
            _ => false,
        };
        if !accepted_scheme {
            return Err("base_url scheme or host is not allowed");
        }
        if base_url.query().is_some() || base_url.fragment().is_some() {
            return Err("base_url has a query or fragment");
        }
        if attachments.max_upload_bytes > u32::MAX as u64 {
            return Err("max_upload_bytes exceeds the remote attachment limit");
        }
        Ok(Self {
            base_url,
            max_upload_bytes: or_default(
                attachments.max_upload_bytes,
                BACKEND_DEFAULT_MAX_UPLOAD_BYTES,
            ),
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
                    tracing::warn!(reason, "ignoring unusable attachment storage offer");
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
        }
    }
}

#[cfg(test)]
mod tests;
