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

/// The attachment service a deployment offers.
#[derive(Clone, Debug, uniffi::Record)]
pub struct AttachmentsConfiguration {
    pub base_url: String,
    pub max_upload_bytes: u64,
    pub retention_seconds: u64,
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
    /// Omitted when the deployment offers no attachment service.
    pub attachments: Option<AttachmentsConfiguration>,
    /// The fields a new conversation registers, in published order.
    pub application_components: Vec<crate::ApplicationComponentDefinition>,
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
            attachments: value
                .attachments
                .as_ref()
                .map(|attachments| AttachmentsConfiguration {
                    base_url: attachments.base_url.clone(),
                    max_upload_bytes: attachments.max_upload_bytes,
                    retention_seconds: attachments.retention_seconds,
                }),
            application_components: value
                .application_components
                .iter()
                .map(Into::into)
                .collect(),
        }
    }
}

/// Private native record sample. @xmtp-internal
/// This checks FFI projection, not backend configuration acceptance.
/// The envelope limit uses the native pointer width. The upload limit is u64
/// on every ABI and stays above the exact JavaScript number range.
#[cfg(all(feature = "conformance", not(target_arch = "wasm32")))]
#[xmtp_macro::sdk_export(native_only)]
pub fn sdk_conformance_server_configuration_sample(
    commit_log_enabled: Option<bool>,
) -> ServerConfiguration {
    use xmtp_mls::mls_common::app_data::fields::MetadataComponentType;
    use xmtp_proto::xmtp::mls::message_contents::metadata_policy::MetadataBasePolicy;
    let allow = Some(config::MetadataPolicy::Base(
        MetadataBasePolicy::Allow.into(),
    ));
    let core = config::ServerConfiguration {
        identifier: "kotlin-configuration-probe".into(),
        server_version: "2.3.4".into(),
        min_libxmtp_version: "1.2.3".into(),
        auth: config::AuthConfiguration {
            enabled: true,
            keys: vec![config::SigningKeyDescription {
                kid: "probe-key".into(),
                alg: "ES256".into(),
            }],
            audiences: vec!["probe-audience".into()],
            issuers: vec!["probe-issuer".into()],
            required_scopes: vec!["message:read".into(), "message:write".into()],
        },
        retention: config::RetentionConfiguration {
            group_message_seconds: 31,
            welcome_seconds: 32,
            key_package_seconds: 33,
        },
        mls: config::MlsConfiguration {
            max_group_members: 21,
            max_installations_per_inbox: 22,
            commit_log_enabled,
        },
        smart_contract_wallet_chains: vec!["eip155:1".into(), "eip155:31337".into()],
        limits: config::LimitsConfiguration {
            max_envelope_bytes: usize::MAX,
            max_request_bytes: 2,
            max_response_bytes: 3,
            max_publish_topics: 4,
            max_query_topics: 5,
            max_query_limit: 6,
            default_query_limit: 7,
            max_newest_metadata_topics: 8,
            max_newest_full_topics: 9,
            max_update_adds: 10,
            max_update_removes: 11,
            max_stream_topics: 12,
            max_static_topics: 13,
            max_lookup_identifiers: 14,
            max_scw_signatures: 15,
            max_identity_entries: 16,
            max_update_frames_per_second: 2_147_483_648,
            max_update_burst: 18,
            max_ping_frames_per_second: 19,
            max_ping_burst: 20,
        },
        attachments: Some(config::AttachmentsConfiguration {
            base_url: "https://files.example/v1/".into(),
            max_upload_bytes: 9_007_199_254_741_025,
            retention_seconds: 41,
        }),
        application_components: vec![config::ApplicationComponentDefinition {
            component_id: 0xC321,
            name: "configuration_host_probe".into(),
            component_type: MetadataComponentType::String
                .tag()
                .expect("known string type")
                .into(),
            permissions: config::ComponentPermissions {
                insert: allow.clone(),
                update: allow.clone(),
                delete: allow,
            },
            in_groups: true,
            in_dms: false,
        }],
    };
    ServerConfiguration::from(&core)
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
            attachments: Some(config::AttachmentsConfiguration {
                base_url: "https://files.example/v1/".into(),
                max_upload_bytes: (1 << 53) + 1,
                retention_seconds: 604_800,
            }),
            ..Default::default()
        };
        let public = ServerConfiguration::from(&core);
        let attachments = public.attachments.as_ref().expect("attachments offered");
        assert_eq!(attachments.base_url, "https://files.example/v1/");
        assert_eq!(attachments.max_upload_bytes, 9_007_199_254_740_993);
        assert_eq!(attachments.retention_seconds, 604_800);
        let not_offered = config::ServerConfiguration {
            attachments: None,
            ..core.clone()
        };
        assert!(
            ServerConfiguration::from(&not_offered)
                .attachments
                .is_none()
        );
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

    /// Each definition keeps its ID, name, type, policy tree and conversation
    /// kinds. A type or policy tag the SDK does not know keeps its tag, and a
    /// missing policy reads as tag 0.
    #[xmtp_common::test(unwrap_try = true)]
    fn application_components_keep_every_definition_field() {
        use crate::{
            ApplicationComponentDefinition, ComponentPermissions, MetadataBasePolicy as B,
            MetadataComponentType as T, MetadataKeyType, MetadataPolicy as P, MetadataScalarType,
        };
        use config::MetadataPolicy as C;
        let core = config::ServerConfiguration {
            application_components: vec![
                config::ApplicationComponentDefinition {
                    component_id: 0xC002,
                    name: "nickname".into(),
                    component_type: 7,
                    permissions: config::ComponentPermissions {
                        insert: Some(C::Base(5)),
                        update: Some(C::And(vec![
                            C::Base(3),
                            C::Any(vec![C::Base(4), C::Base(77)]),
                        ])),
                        delete: None,
                    },
                    in_groups: true,
                    in_dms: false,
                },
                config::ApplicationComponentDefinition {
                    component_id: 0xC001,
                    name: "later".into(),
                    component_type: 99,
                    permissions: config::ComponentPermissions {
                        insert: Some(C::Base(1)),
                        update: Some(C::Base(2)),
                        delete: Some(C::Base(0)),
                    },
                    in_groups: false,
                    in_dms: true,
                },
            ],
            ..Default::default()
        };
        assert_eq!(
            ServerConfiguration::from(&core).application_components,
            [
                ApplicationComponentDefinition {
                    component_id: 0xC002,
                    name: "nickname".into(),
                    component_type: T::Map {
                        key_type: MetadataKeyType::InboxId,
                        value_type: MetadataScalarType::String,
                    },
                    permissions: ComponentPermissions {
                        insert: P::Base(B::AllowIfSelfOrNonMember),
                        update: P::And(vec![
                            P::Base(B::AllowIfAdmin),
                            P::Any(vec![
                                P::Base(B::AllowIfSuperAdmin),
                                P::Base(B::Unknown { tag: 77 }),
                            ]),
                        ]),
                        delete: P::Base(B::Unknown { tag: 0 }),
                    },
                    in_groups: true,
                    in_dms: false,
                },
                ApplicationComponentDefinition {
                    component_id: 0xC001,
                    name: "later".into(),
                    component_type: T::Unknown { tag: 99 },
                    permissions: ComponentPermissions {
                        insert: P::Base(B::Allow),
                        update: P::Base(B::Deny),
                        delete: P::Base(B::Unknown { tag: 0 }),
                    },
                    in_groups: false,
                    in_dms: true,
                },
            ]
        );
    }
}
