//! The configuration a deployment publishes, read and applied through the
//! public API without the client's internals.

#[cfg(not(target_arch = "wasm32"))]
mod applied_tests;

#[cfg(not(target_arch = "wasm32"))]
mod backend_tests {

    use std::sync::Arc;
    use xmtp_api_backend::{BackendClient, TrackedStatsClient};
    use xmtp_backend::test_support::TestServer;

    use xmtp_proto::api::ToBoxedClient;

    /// A deployment that publishes values no compiled default would produce.
    const DISTINCT: &str = r#"
[server]
identifier = "org.example.distinct"

[limits]
max_query_topics = 17

[mls]
max_group_members = 23

[handles]
min_name_length = 5

[chains]
"eip155:1" = "http://127.0.0.1:8545"
"eip155:8453" = "http://127.0.0.1:8545"

[[application_components]]
component_id = 0xC001
name = "USER_PRONOUNS"
component_type = "tls_map_inbox_id_string"
insert_policy = "allow_if_self_or_non_member"
update_policy = "allow_if_self_or_non_member"
delete_policy = "allow_if_admin"
in_groups = true
in_dms = true

[[application_components]]
component_id = 0xC000
name = "topic"
component_type = "string"
insert_policy = "allow"
update_policy = "allow_if_admin"
delete_policy = "deny"
in_groups = true
in_dms = false
"#;

    // The configuration read is unauthenticated, so a
    // client with an auth callback never invokes it for this call.
    // verifies: CONF-029
    #[xmtp_common::test(unwrap_try = true)]
    async fn reading_the_configuration_never_invokes_the_auth_callback() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use xmtp_api_backend::{AuthCallback, AuthMiddleware, Credential};

        struct CountingCallback(Arc<AtomicUsize>);

        #[xmtp_common::async_trait]
        impl AuthCallback for CountingCallback {
            async fn on_auth_required(&self) -> Result<Credential, xmtp_common::BoxDynError> {
                self.0.fetch_add(1, Ordering::SeqCst);
                Err("this test never supplies a credential".into())
            }
        }

        let backend = TestServer::from_toml(DISTINCT).await?;
        let calls = Arc::new(AtomicUsize::new(0));
        let transport = xmtp_api_grpc::GrpcClient::create(backend.url.parse().unwrap()).unwrap();
        let transport = AuthMiddleware::new(
            transport,
            Some(Arc::new(CountingCallback(calls.clone())) as Arc<dyn AuthCallback>),
            None,
        )
        .arced();
        let api = xmtp_api::ApiClientWrapper::new(
            Arc::new(TrackedStatsClient::new(BackendClient::new(transport))),
            Default::default(),
        );

        let configuration = api.get_configuration().await?;
        assert_eq!(configuration.identifier, "org.example.distinct");
        assert_eq!(calls.load(Ordering::SeqCst), 0);

        backend.stop().await?;
    }
}
