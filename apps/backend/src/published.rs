//! The deployment settings published by `GetConfiguration`. `server::initialize`
//! builds them once, from the validated configuration and the signing keys
//! loaded at startup, and checks their size before storage starts.

use crate::{
    api,
    config::{
        Config, ConfigError, application_components::ApplicationComponentConfig,
        auth::SigningAlgorithm,
    },
};

/// The published configuration must stay small enough for one cheap response.
pub(crate) const MAX_CONFIGURATION_RESPONSE_BYTES: usize = 64 * 1024;

/// Build the configuration published to clients. It is assembled once at
/// startup and never changes while the process runs.
///
/// `jwks_keys` carries the `(kid, alg)` pairs of the key set fetched at
/// startup; it is empty for an inline-key or credential-free deployment,
/// where the keys come from the configuration itself. Nothing secret
/// belongs here: no JWKS URL, no key material, no chain RPC URL, and no
/// database, telemetry, or listener setting.
// implements: CONF-011, CONF-070
pub(crate) fn response(
    config: &Config,
    jwks_keys: &[(String, SigningAlgorithm)],
) -> api::GetConfigurationResponse {
    let limits = &config.limits;
    // Published limits are narrower wire types than the configured ones.
    // Saturating keeps an out-of-range configured value bounded rather
    // than wrapping it into a smaller, wrong promise.
    let count = |value: usize| u32::try_from(value).unwrap_or(u32::MAX);
    api::GetConfigurationResponse {
        identifier: config.server.identifier().to_owned(),
        server_version: env!("CARGO_PKG_VERSION").to_owned(),
        min_libxmtp_version: config
            .server
            .min_libxmtp_version
            .clone()
            .unwrap_or_default(),
        auth: Some(auth_configuration(config, jwks_keys)),
        retention: Some(api::RetentionConfiguration {
            group_message_seconds: config.retention.group_message_seconds,
            welcome_seconds: config.retention.welcome_seconds,
            key_package_seconds: config.retention.key_package_seconds,
        }),
        limits: Some(api::LimitsConfiguration {
            max_envelope_bytes: limits.max_envelope_bytes as u64,
            max_request_bytes: limits.max_request_bytes as u64,
            max_response_bytes: limits.max_response_bytes as u64,
            max_publish_topics: count(limits.max_publish_topics),
            max_query_topics: count(limits.max_query_topics),
            max_query_limit: count(limits.max_query_limit),
            default_query_limit: count(limits.default_query_limit),
            max_newest_metadata_topics: count(limits.max_newest_metadata_topics),
            max_newest_full_topics: count(limits.max_newest_full_topics),
            max_update_adds: count(limits.max_update_adds),
            max_update_removes: count(limits.max_update_removes),
            max_stream_topics: count(limits.max_stream_topics),
            max_static_topics: count(limits.max_static_topics),
            max_lookup_identifiers: count(limits.max_lookup_identifiers),
            max_scw_signatures: count(limits.max_scw_signatures),
            max_identity_entries: count(limits.max_identity_entries),
            max_update_frames_per_second: limits.max_update_frames_per_second,
            max_update_burst: limits.max_update_burst,
            max_ping_frames_per_second: limits.max_ping_frames_per_second,
            max_ping_burst: limits.max_ping_burst,
        }),
        mls: Some(api::MlsConfiguration {
            max_group_members: config.mls.max_group_members,
            max_installations_per_inbox: config.mls.max_installations_per_inbox,
            commit_log_enabled: Some(config.mls.commit_log_enabled),
        }),
        handles: Some(api::HandlesConfiguration {
            min_name_length: config.handles.min_name_length,
        }),
        smart_contract_wallet_chains: config.chains.keys().cloned().collect(),
        attachments: config
            .attachments
            .as_ref()
            .map(|attachments| api::AttachmentsConfiguration {
                base_url: attachments.base_url.clone(),
                max_upload_bytes: attachments.upload_ceiling(),
                retention_seconds: attachments.retention_seconds.unwrap_or_default(),
            }),
        application_components: application_components(&config.application_components),
    }
}

/// Publish what a client needs to hold a credential, and nothing else.
/// A credential-free deployment publishes an otherwise empty section.
// implements: CONF-068
fn auth_configuration(
    config: &Config,
    jwks_keys: &[(String, SigningAlgorithm)],
) -> api::AuthConfiguration {
    let Some(auth) = config.auth.as_ref().filter(|auth| auth.is_enabled()) else {
        return api::AuthConfiguration::default();
    };
    let keys = match &auth.keys {
        Some(keys) => keys
            .iter()
            .map(|key| api::auth_configuration::SigningKey {
                kid: key.kid.clone(),
                alg: key.alg.clone(),
            })
            .collect(),
        None => jwks_keys
            .iter()
            .map(|(kid, alg)| api::auth_configuration::SigningKey {
                kid: kid.clone(),
                alg: alg.name().to_owned(),
            })
            .collect(),
    };
    api::AuthConfiguration {
        enabled: true,
        keys,
        audiences: auth.audiences.clone().unwrap_or_default(),
        issuers: auth.issuers.clone().unwrap_or_default(),
        required_scopes: auth.required_scopes.clone(),
    }
}

/// The published catalogue, sorted by `component_id` so every client builds
/// the same snapshot whatever order the file lists them in.
// implements: CONF-079
fn application_components(
    components: &[ApplicationComponentConfig],
) -> Vec<api::ApplicationComponentDefinition> {
    let mut published: Vec<api::ApplicationComponentDefinition> = components
        .iter()
        .map(|component| xmtp_configuration::ApplicationComponentDefinition::from(component).into())
        .collect();
    published.sort_by_key(|definition| definition.component_id);
    published
}

/// Keep the published configuration inside one small response. An oversized
/// one is a configuration mistake, not a runtime condition, so it stops
/// startup rather than failing every client that asks for it.
// implements: CONF-009
pub(crate) fn check_size(response: &api::GetConfigurationResponse) -> Result<(), ConfigError> {
    if prost::Message::encoded_len(response) > MAX_CONFIGURATION_RESPONSE_BYTES {
        return Err(ConfigError::Invalid {
            field: "configuration response",
            reason: "must not exceed 64 KiB once encoded",
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests;
