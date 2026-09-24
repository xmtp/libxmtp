use xmtp_configuration as config;

#[derive(Clone, Debug, uniffi::Record)]
pub struct SigningKeyDescription {
    pub kid: String,
    pub alg: String,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct AuthConfiguration {
    pub enabled: bool,
    pub keys: Vec<SigningKeyDescription>,
    pub audiences: Vec<String>,
    pub issuers: Vec<String>,
    pub required_scopes: Vec<String>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct RetentionConfiguration {
    pub group_message_seconds: u64,
    pub welcome_seconds: u64,
    pub key_package_seconds: u64,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct LimitsConfiguration {
    pub max_envelope_bytes: u64,
    pub max_request_bytes: u64,
    pub max_response_bytes: u64,
    pub max_publish_topics: u32,
    pub max_query_topics: u32,
    pub max_query_limit: u32,
    pub default_query_limit: u32,
    pub max_newest_metadata_topics: u32,
    pub max_newest_full_topics: u32,
    pub max_update_adds: u32,
    pub max_update_removes: u32,
    pub max_stream_topics: u32,
    pub max_static_topics: u32,
    pub max_lookup_identifiers: u32,
    pub max_scw_signatures: u32,
    pub max_identity_entries: u32,
    pub max_update_frames_per_second: u32,
    pub max_update_burst: u32,
    pub max_ping_frames_per_second: u32,
    pub max_ping_burst: u32,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct MlsConfiguration {
    pub max_group_members: u64,
    pub max_installations_per_inbox: u64,
    pub commit_log_enabled: Option<bool>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct ServerConfiguration {
    pub identifier: String,
    pub server_version: String,
    pub min_libxmtp_version: String,
    pub auth: AuthConfiguration,
    pub retention: RetentionConfiguration,
    pub limits: LimitsConfiguration,
    pub mls: MlsConfiguration,
    pub smart_contract_wallet_chains: Vec<String>,
}

impl From<&config::ServerConfiguration> for ServerConfiguration {
    fn from(value: &config::ServerConfiguration) -> Self {
        let limits = &value.limits;
        Self {
            identifier: value.identifier.clone(),
            server_version: value.server_version.clone(),
            min_libxmtp_version: value.min_libxmtp_version.clone(),
            auth: AuthConfiguration {
                enabled: value.auth.enabled,
                keys: value
                    .auth
                    .keys
                    .iter()
                    .map(|key| SigningKeyDescription {
                        kid: key.kid.clone(),
                        alg: key.alg.clone(),
                    })
                    .collect(),
                audiences: value.auth.audiences.clone(),
                issuers: value.auth.issuers.clone(),
                required_scopes: value.auth.required_scopes.clone(),
            },
            retention: RetentionConfiguration {
                group_message_seconds: value.retention.group_message_seconds,
                welcome_seconds: value.retention.welcome_seconds,
                key_package_seconds: value.retention.key_package_seconds,
            },
            limits: LimitsConfiguration {
                max_envelope_bytes: limits.max_envelope_bytes as u64,
                max_request_bytes: limits.max_request_bytes as u64,
                max_response_bytes: limits.max_response_bytes as u64,
                max_publish_topics: limits.max_publish_topics as u32,
                max_query_topics: limits.max_query_topics as u32,
                max_query_limit: limits.max_query_limit as u32,
                default_query_limit: limits.default_query_limit as u32,
                max_newest_metadata_topics: limits.max_newest_metadata_topics as u32,
                max_newest_full_topics: limits.max_newest_full_topics as u32,
                max_update_adds: limits.max_update_adds as u32,
                max_update_removes: limits.max_update_removes as u32,
                max_stream_topics: limits.max_stream_topics as u32,
                max_static_topics: limits.max_static_topics as u32,
                max_lookup_identifiers: limits.max_lookup_identifiers as u32,
                max_scw_signatures: limits.max_scw_signatures as u32,
                max_identity_entries: limits.max_identity_entries as u32,
                max_update_frames_per_second: limits.max_update_frames_per_second,
                max_update_burst: limits.max_update_burst,
                max_ping_frames_per_second: limits.max_ping_frames_per_second,
                max_ping_burst: limits.max_ping_burst,
            },
            mls: MlsConfiguration {
                max_group_members: value.mls.max_group_members as u64,
                max_installations_per_inbox: value.mls.max_installations_per_inbox as u64,
                commit_log_enabled: value.mls.commit_log_enabled,
            },
            smart_contract_wallet_chains: value.smart_contract_wallet_chains.clone(),
        }
    }
}
