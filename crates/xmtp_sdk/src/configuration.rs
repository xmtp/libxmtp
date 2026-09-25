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
    pub max_publish_topics: u64,
    pub max_query_topics: u64,
    pub max_query_limit: u64,
    pub default_query_limit: u64,
    pub max_newest_metadata_topics: u64,
    pub max_newest_full_topics: u64,
    pub max_update_adds: u64,
    pub max_update_removes: u64,
    pub max_stream_topics: u64,
    pub max_static_topics: u64,
    pub max_lookup_identifiers: u64,
    pub max_scw_signatures: u64,
    pub max_identity_entries: u64,
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
                max_publish_topics: limits.max_publish_topics as u64,
                max_query_topics: limits.max_query_topics as u64,
                max_query_limit: limits.max_query_limit as u64,
                default_query_limit: limits.default_query_limit as u64,
                max_newest_metadata_topics: limits.max_newest_metadata_topics as u64,
                max_newest_full_topics: limits.max_newest_full_topics as u64,
                max_update_adds: limits.max_update_adds as u64,
                max_update_removes: limits.max_update_removes as u64,
                max_stream_topics: limits.max_stream_topics as u64,
                max_static_topics: limits.max_static_topics as u64,
                max_lookup_identifiers: limits.max_lookup_identifiers as u64,
                max_scw_signatures: limits.max_scw_signatures as u64,
                max_identity_entries: limits.max_identity_entries as u64,
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

#[cfg(all(test, target_pointer_width = "64"))]
mod tests {
    use super::*;

    #[xmtp_common::test(unwrap_try = true)]
    fn wide_limits_keep_their_value() {
        let wide = 1_usize << 32;
        let core = config::ServerConfiguration {
            limits: config::LimitsConfiguration {
                max_publish_topics: wide,
                max_query_topics: wide,
                max_query_limit: wide,
                default_query_limit: wide,
                max_newest_metadata_topics: wide,
                max_newest_full_topics: wide,
                max_update_adds: wide,
                max_update_removes: wide,
                max_stream_topics: wide,
                max_static_topics: wide,
                max_lookup_identifiers: wide,
                max_scw_signatures: wide,
                max_identity_entries: wide,
                ..Default::default()
            },
            ..Default::default()
        };
        let limits = ServerConfiguration::from(&core).limits;
        for value in [
            limits.max_publish_topics,
            limits.max_query_topics,
            limits.max_query_limit,
            limits.default_query_limit,
            limits.max_newest_metadata_topics,
            limits.max_newest_full_topics,
            limits.max_update_adds,
            limits.max_update_removes,
            limits.max_stream_topics,
            limits.max_static_topics,
            limits.max_lookup_identifiers,
            limits.max_scw_signatures,
            limits.max_identity_entries,
        ] {
            assert_eq!(value, wide as u64);
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn server_configuration_preserves_all_published_fields() {
        let core = config::ServerConfiguration {
            identifier: "test-backend".into(),
            server_version: "2.3.4".into(),
            min_libxmtp_version: "1.2.3".into(),
            auth: config::AuthConfiguration {
                enabled: true,
                keys: vec![config::SigningKeyDescription {
                    kid: "key".into(),
                    alg: "EdDSA".into(),
                }],
                audiences: vec!["audience".into()],
                issuers: vec!["issuer".into()],
                required_scopes: vec!["message:write".into()],
            },
            retention: config::RetentionConfiguration {
                group_message_seconds: 11,
                welcome_seconds: 12,
                key_package_seconds: 13,
            },
            mls: config::MlsConfiguration {
                max_group_members: 321,
                max_installations_per_inbox: 17,
                commit_log_enabled: Some(false),
            },
            smart_contract_wallet_chains: vec!["eip155:1".into()],
            ..Default::default()
        };
        let public = ServerConfiguration::from(&core);
        assert_eq!(public.identifier, core.identifier);
        assert_eq!(public.server_version, core.server_version);
        assert_eq!(public.min_libxmtp_version, core.min_libxmtp_version);
        assert_eq!(public.auth.enabled, core.auth.enabled);
        assert_eq!(public.auth.keys.len(), 1);
        assert_eq!(public.auth.keys[0].kid, core.auth.keys[0].kid);
        assert_eq!(public.auth.keys[0].alg, core.auth.keys[0].alg);
        assert_eq!(public.auth.audiences, core.auth.audiences);
        assert_eq!(public.auth.issuers, core.auth.issuers);
        assert_eq!(public.auth.required_scopes, core.auth.required_scopes);
        assert_eq!(
            public.retention.group_message_seconds,
            core.retention.group_message_seconds
        );
        assert_eq!(
            public.retention.welcome_seconds,
            core.retention.welcome_seconds
        );
        assert_eq!(
            public.retention.key_package_seconds,
            core.retention.key_package_seconds
        );
        assert_eq!(
            public.mls.max_group_members,
            core.mls.max_group_members as u64
        );
        assert_eq!(
            public.mls.max_installations_per_inbox,
            core.mls.max_installations_per_inbox as u64
        );
        assert_eq!(public.mls.commit_log_enabled, core.mls.commit_log_enabled);
        assert_eq!(
            public.smart_contract_wallet_chains,
            core.smart_contract_wallet_chains
        );
        macro_rules! limit {
            ($($field:ident),+) => {
                $(assert_eq!(public.limits.$field, core.limits.$field as u64);)+
            };
        }
        limit!(
            max_envelope_bytes,
            max_request_bytes,
            max_response_bytes,
            max_publish_topics,
            max_query_topics,
            max_query_limit,
            default_query_limit,
            max_newest_metadata_topics,
            max_newest_full_topics,
            max_update_adds,
            max_update_removes,
            max_stream_topics,
            max_static_topics,
            max_lookup_identifiers,
            max_scw_signatures,
            max_identity_entries
        );
        assert_eq!(
            public.limits.max_update_frames_per_second,
            core.limits.max_update_frames_per_second
        );
        assert_eq!(public.limits.max_update_burst, core.limits.max_update_burst);
        assert_eq!(
            public.limits.max_ping_frames_per_second,
            core.limits.max_ping_frames_per_second
        );
        assert_eq!(public.limits.max_ping_burst, core.limits.max_ping_burst);
    }
}
