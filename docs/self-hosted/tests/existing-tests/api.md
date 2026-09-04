# API test index

[← Test inventory](../existing-tests.md) · [Requirements](../existing-requirements.md)

| File | Qualified test | Form / gates / cases | Requirements |
| --- | --- | --- | --- |
| `crates/xmtp_api/src/identity.rs` | `identity::tests::publish_identity_update` | custom async | `API-REQ-001` |
| `crates/xmtp_api/src/identity.rs` | `identity::tests::publish_identity_update_wrapper_returns_option_cursor` | custom async; `unwrap_try` | `API-REQ-001` |
| `crates/xmtp_api/src/identity.rs` | `identity::tests::get_identity_update_v2` | custom async | `API-REQ-002` |
| `crates/xmtp_api/src/identity.rs` | `identity::tests::get_inbox_ids` | custom async | `API-REQ-003` |
| `crates/xmtp_api/src/mls.rs` | `mls::tests::test_upload_key_package` | custom async | `API-REQ-004` |
| `crates/xmtp_api/src/mls.rs` | `mls::tests::test_fetch_key_packages` | custom async | `API-REQ-005` |
| `crates/xmtp_api/src/mls.rs` | `mls::tests::test_read_group_messages_single_page` | custom async | `API-REQ-006` |
| `crates/xmtp_api/src/mls.rs` | `mls::tests::test_read_group_messages_single_page_xactly_100_results` | custom async | `API-REQ-006` |
| `crates/xmtp_api/src/mls.rs` | `mls::tests::test_read_topic_multi_page` | custom async | `API-REQ-006` |
| `crates/xmtp_api/src/mls.rs` | `mls::tests::it_retries_twice_then_succeeds` | custom async | `API-REQ-007` |
| `crates/xmtp_api/src/mls.rs` | `mls::tests::it_should_rate_limit` | custom async; ignored; one request per minute; second query elapsed time exceeds 60 seconds | `API-REQ-008` |
| `crates/xmtp_api/src/mls.rs` | `mls::tests::it_should_allow_large_payloads` | custom async; ignored on wasm; sends ten 900,000-byte chunks and requires query success with one result; no returned-payload integrity assertion | `API-REQ-008` |
| `crates/xmtp_api/src/mls.rs` | `mls::tests::test_publish_commit_log_batching_with_local_server` | custom async; 11 requests; accepts success or any error except the specific over-10 single-batch error | `API-REQ-009` |
| `crates/xmtp_api/src/mls.rs` | `mls::tests::test_query_commit_log_batching_with_local_server` | custom async; 21 requests; call must succeed; response count only constrained to at most 21 | `API-REQ-009` |
| `crates/xmtp_api_d14n/src/endpoints/v3/identity/get_identity_updates_v2.rs` | `endpoints::v3::identity::get_identity_updates_v2::test::test_grpc_endpoint_returns_correct_path` | custom sync | `API-REQ-010` |
| `crates/xmtp_api_d14n/src/endpoints/v3/identity/get_identity_updates_v2.rs` | `endpoints::v3::identity::get_identity_updates_v2::test::test_get_identity_updates_v2` | custom async | `API-REQ-011` |
| `crates/xmtp_api_d14n/src/endpoints/v3/identity/get_inbox_ids.rs` | `endpoints::v3::identity::get_inbox_ids::test::test_grpc_endpoint_returns_correct_path` | custom sync | `API-REQ-010` |
| `crates/xmtp_api_d14n/src/endpoints/v3/identity/get_inbox_ids.rs` | `endpoints::v3::identity::get_inbox_ids::test::test_get_inbox_ids` | custom async | `API-REQ-011` |
| `crates/xmtp_api_d14n/src/endpoints/v3/identity/publish_identity_update.rs` | `endpoints::v3::identity::publish_identity_update::test::test_grpc_endpoint_returns_correct_path` | custom sync | `API-REQ-010` |
| `crates/xmtp_api_d14n/src/endpoints/v3/identity/publish_identity_update.rs` | `endpoints::v3::identity::publish_identity_update::test::test_publish_identity_update` | custom async | `API-REQ-011` |
| `crates/xmtp_api_d14n/src/endpoints/v3/identity/verify_smart_contract_wallet_signatures.rs` | `endpoints::v3::identity::verify_smart_contract_wallet_signatures::test::test_grpc_endpoint_returns_correct_path` | custom sync | `API-REQ-010` |
| `crates/xmtp_api_d14n/src/endpoints/v3/identity/verify_smart_contract_wallet_signatures.rs` | `endpoints::v3::identity::verify_smart_contract_wallet_signatures::test::test_verify_smart_contract_wallet_signatures` | custom async | `API-REQ-011` |
| `crates/xmtp_api_d14n/src/endpoints/v3/mls/fetch_key_packages.rs` | `endpoints::v3::mls::fetch_key_packages::test::test_grpc_endpoint_returns_correct_path` | custom sync | `API-REQ-010` |
| `crates/xmtp_api_d14n/src/endpoints/v3/mls/fetch_key_packages.rs` | `endpoints::v3::mls::fetch_key_packages::test::test_fetch_key_packages` | custom async | `API-REQ-012` |
| `crates/xmtp_api_d14n/src/endpoints/v3/mls/get_newest_group_message.rs` | `endpoints::v3::mls::get_newest_group_message::test::test_grpc_endpoint_returns_correct_path` | custom sync | `API-REQ-010` |
| `crates/xmtp_api_d14n/src/endpoints/v3/mls/get_newest_group_message.rs` | `endpoints::v3::mls::get_newest_group_message::test::test_get_newest_group_message` | custom async | `API-REQ-015` |
| `crates/xmtp_api_d14n/src/endpoints/v3/mls/get_newest_group_message.rs` | `endpoints::v3::mls::get_newest_group_message::test::test_get_newest_group_message_builder` | custom sync | `API-REQ-015` |
| `crates/xmtp_api_d14n/src/endpoints/v3/mls/get_newest_group_message.rs` | `endpoints::v3::mls::get_newest_group_message::test::test_get_newest_group_message_builder_with_content` | custom sync | `API-REQ-015` |
| `crates/xmtp_api_d14n/src/endpoints/v3/mls/get_newest_group_message.rs` | `endpoints::v3::mls::get_newest_group_message::test::test_get_newest_group_message_endpoints` | custom sync | `API-REQ-010`, `API-REQ-015` |
| `crates/xmtp_api_d14n/src/endpoints/v3/mls/publish_commit_log.rs` | `endpoints::v3::mls::publish_commit_log::test::test_grpc_endpoint_returns_correct_path` | custom sync | `API-REQ-010` |
| `crates/xmtp_api_d14n/src/endpoints/v3/mls/publish_commit_log.rs` | `endpoints::v3::mls::publish_commit_log::test::test_publish_commit_log` | custom async | `API-REQ-013` |
| `crates/xmtp_api_d14n/src/endpoints/v3/mls/query_commit_log.rs` | `endpoints::v3::mls::query_commit_log::test::test_grpc_endpoint_returns_correct_path` | custom sync | `API-REQ-010` |
| `crates/xmtp_api_d14n/src/endpoints/v3/mls/query_commit_log.rs` | `endpoints::v3::mls::query_commit_log::test::test_query_commit_log` | custom async | `API-REQ-012` |
| `crates/xmtp_api_d14n/src/endpoints/v3/mls/query_group_messages.rs` | `endpoints::v3::mls::query_group_messages::test::test_grpc_endpoint_returns_correct_path` | custom sync | `API-REQ-010` |
| `crates/xmtp_api_d14n/src/endpoints/v3/mls/query_group_messages.rs` | `endpoints::v3::mls::query_group_messages::test::test_query_group_messages` | custom async | `API-REQ-012` |
| `crates/xmtp_api_d14n/src/endpoints/v3/mls/query_welcome_messages.rs` | `endpoints::v3::mls::query_welcome_messages::test::test_grpc_endpoint_returns_correct_path` | custom sync | `API-REQ-010` |
| `crates/xmtp_api_d14n/src/endpoints/v3/mls/query_welcome_messages.rs` | `endpoints::v3::mls::query_welcome_messages::test::test_query_welcome_messages` | custom async | `API-REQ-012` |
| `crates/xmtp_api_d14n/src/endpoints/v3/mls/send_group_messages.rs` | `endpoints::v3::mls::send_group_messages::test::test_grpc_endpoint_returns_correct_path` | custom sync | `API-REQ-010` |
| `crates/xmtp_api_d14n/src/endpoints/v3/mls/send_group_messages.rs` | `endpoints::v3::mls::send_group_messages::test::test_send_group_messages` | custom async | `API-REQ-013` |
| `crates/xmtp_api_d14n/src/endpoints/v3/mls/send_welcome_messages.rs` | `endpoints::v3::mls::send_welcome_messages::test::test_grpc_endpoint_returns_correct_path` | custom sync | `API-REQ-010` |
| `crates/xmtp_api_d14n/src/endpoints/v3/mls/send_welcome_messages.rs` | `endpoints::v3::mls::send_welcome_messages::test::test_send_welcome_messages` | custom async | `API-REQ-013` |
| `crates/xmtp_api_d14n/src/endpoints/v3/mls/subscribe_group_messages.rs` | `endpoints::v3::mls::subscribe_group_messages::test::test_grpc_endpoint_returns_correct_path` | custom sync | `API-REQ-010` |
| `crates/xmtp_api_d14n/src/endpoints/v3/mls/subscribe_group_messages.rs` | `endpoints::v3::mls::subscribe_group_messages::test::test_subscribe_envelopes` | custom async | `API-REQ-014` |
| `crates/xmtp_api_d14n/src/endpoints/v3/mls/subscribe_welcome_messages.rs` | `endpoints::v3::mls::subscribe_welcome_messages::test::test_grpc_endpoint_returns_correct_path` | custom sync | `API-REQ-010` |
| `crates/xmtp_api_d14n/src/endpoints/v3/mls/subscribe_welcome_messages.rs` | `endpoints::v3::mls::subscribe_welcome_messages::test::test_subscribe_envelopes` | custom async | `API-REQ-014` |
| `crates/xmtp_api_d14n/src/endpoints/v3/mls/upload_key_package.rs` | `endpoints::v3::mls::upload_key_package::test::test_grpc_endpoint_returns_correct_path` | custom sync | `API-REQ-010` |
| `crates/xmtp_api_d14n/src/endpoints/v3/mls/upload_key_package.rs` | `endpoints::v3::mls::upload_key_package::test::test_upload_key_package` | custom async | `API-REQ-013` |
| `crates/xmtp_api_d14n/src/endpoints/d14n/fetch_d14n_cutover.rs` | `endpoints::d14n::fetch_d14n_cutover::test::test_grpc_endpoint_returns_correct_path` | custom sync | `API-REQ-010` |
| `crates/xmtp_api_d14n/src/endpoints/d14n/fetch_d14n_cutover.rs` | `endpoints::d14n::fetch_d14n_cutover::test::test_fetch_d14n_cutover` | custom async; ignored | `API-REQ-016` |
| `crates/xmtp_api_d14n/src/endpoints/d14n/get_inbox_ids.rs` | `endpoints::d14n::get_inbox_ids::test::test_grpc_endpoint_returns_correct_path` | custom sync | `API-REQ-010` |
| `crates/xmtp_api_d14n/src/endpoints/d14n/get_inbox_ids.rs` | `endpoints::d14n::get_inbox_ids::test::test_get_inbox_ids` | custom async | `API-REQ-016` |
| `crates/xmtp_api_d14n/src/endpoints/d14n/get_newest_envelopes.rs` | `endpoints::d14n::get_newest_envelopes::test::test_grpc_endpoint_returns_correct_path` | custom sync | `API-REQ-010` |
| `crates/xmtp_api_d14n/src/endpoints/d14n/get_newest_envelopes.rs` | `endpoints::d14n::get_newest_envelopes::test::get_newest_envelopes` | custom async | `API-REQ-016` |
| `crates/xmtp_api_d14n/src/endpoints/d14n/get_nodes.rs` | `endpoints::d14n::get_nodes::test::test_grpc_endpoint_returns_correct_path` | custom sync | `API-REQ-010` |
| `crates/xmtp_api_d14n/src/endpoints/d14n/get_nodes.rs` | `endpoints::d14n::get_nodes::test::test_get_nodes` | custom async | `API-REQ-020` |
| `crates/xmtp_api_d14n/src/endpoints/d14n/get_nodes.rs` | `endpoints::d14n::get_nodes::test::test_get_nodes_unimplemented` | custom async | `API-REQ-020` |
| `crates/xmtp_api_d14n/src/endpoints/d14n/health_check.rs` | `endpoints::d14n::health_check::test::test_health_check` | custom async | `API-REQ-020` |
| `crates/xmtp_api_d14n/src/endpoints/d14n/publish_client_envelopes.rs` | `endpoints::d14n::publish_client_envelopes::test::test_grpc_endpoint_returns_correct_path` | custom sync | `API-REQ-010` |
| `crates/xmtp_api_d14n/src/endpoints/d14n/publish_client_envelopes.rs` | `endpoints::d14n::publish_client_envelopes::test::test_publish_client_envelopes` | custom async | `API-REQ-017` |
| `crates/xmtp_api_d14n/src/endpoints/d14n/query_envelopes.rs` | `endpoints::d14n::query_envelopes::test::test_grpc_endpoint_returns_correct_path` | custom sync | `API-REQ-010` |
| `crates/xmtp_api_d14n/src/endpoints/d14n/query_envelopes.rs` | `endpoints::d14n::query_envelopes::test::test_query_envelopes` | custom async | `API-REQ-018` |
| `crates/xmtp_api_d14n/src/endpoints/d14n/query_envelopes.rs` | `endpoints::d14n::query_envelopes::test::test_query_envelope` | custom async | `API-REQ-018` |
| `crates/xmtp_api_d14n/src/endpoints/d14n/subscribe_topics.rs` | `endpoints::d14n::subscribe_topics::test::test_grpc_endpoint_returns_correct_path` | custom sync | `API-REQ-010` |
| `crates/xmtp_api_d14n/src/endpoints/d14n/subscribe_topics.rs` | `endpoints::d14n::subscribe_topics::test::test_body_encodes_per_topic_filters` | custom sync | `API-REQ-019` |
| `crates/xmtp_api_d14n/src/endpoints/d14n/subscribe_topics.rs` | `endpoints::d14n::subscribe_topics::test::test_empty_filters` | custom sync | `API-REQ-019` |
| `crates/xmtp_api_d14n/src/endpoints/d14n/subscribe_topics.rs` | `endpoints::d14n::subscribe_topics::test::test_subscribe_topics` | custom async; `unwrap_try` | `API-REQ-019` |
| `crates/xmtp_api_d14n/src/middleware/readonly_client.rs` | `middleware::readonly_client::tests::test_forwards_to_inner` | rstest fixture + custom async; `unwrap_try` | `API-REQ-021` |
| `crates/xmtp_api_d14n/src/middleware/readonly_client.rs` | `middleware::readonly_client::tests::test_errors_on_write` | rstest fixture + custom async; `unwrap_try` | `API-REQ-021` |
| `crates/xmtp_api_d14n/src/middleware/read_write_client/client.rs` | `middleware::read_write_client::client::tests::test_writes_when_matches` | rstest fixture + custom async; `unwrap_try` | `API-REQ-022` |
| `crates/xmtp_api_d14n/src/middleware/read_write_client/client.rs` | `middleware::read_write_client::client::tests::test_reads_when_matches` | rstest fixture + custom async; `unwrap_try` | `API-REQ-022` |
| `crates/xmtp_api_d14n/src/middleware/multi_node_client/client.rs` | `middleware::multi_node_client::client::tests::build_multinode_as_d14n` | custom async | `API-REQ-023` |
| `crates/xmtp_api_d14n/src/middleware/multi_node_client/client.rs` | `middleware::multi_node_client::client::tests::build_multinode_as_standalone` | custom async | `API-REQ-023` |
| `crates/xmtp_api_d14n/src/middleware/multi_node_client/client.rs` | `middleware::multi_node_client::client::tests::d14n_request_latest_group_message` | custom async | `API-REQ-023` |
| `crates/xmtp_api_d14n/src/middleware/multi_node_client/client.rs` | `middleware::multi_node_client::client::tests::multinode_request_latest_group_message` | custom async | `API-REQ-023` |
| `crates/xmtp_api_d14n/src/middleware/multi_node_client/gateway_api.rs` | `middleware::multi_node_client::gateway_api::tests::retry_get_nodes_recovers_from_transient_failure` | custom async; `unwrap_try` | `API-REQ-024` |
| `crates/xmtp_api_d14n/src/middleware/multi_node_client/gateway_api.rs` | `middleware::multi_node_client::gateway_api::tests::retry_get_nodes_exhausts_budget_on_repeated_transient_failure` | custom async; `unwrap_try` | `API-REQ-024` |
| `crates/xmtp_api_d14n/src/middleware/multi_node_client/gateway_api.rs` | `middleware::multi_node_client::gateway_api::tests::retry_get_nodes_fails_fast_on_non_retryable_error` | custom async; `unwrap_try` | `API-REQ-024` |
| `crates/xmtp_api_d14n/src/middleware/auth.rs` | `middleware::auth::tests::test_auth_handle` | custom async | `API-REQ-025` |
| `crates/xmtp_api_d14n/src/middleware/auth.rs` | `middleware::auth::tests::test_auth_middleware_no_callback_or_handle` | custom async; native-only macro block | `API-REQ-025` |
| `crates/xmtp_api_d14n/src/middleware/auth.rs` | `middleware::auth::tests::test_auth_middleware_with_no_callback_and_handle` | custom async | `API-REQ-025` |
| `crates/xmtp_api_d14n/src/middleware/auth.rs` | `middleware::auth::tests::test_auth_middleware_with_callback_and_no_handle` | custom async | `API-REQ-025` |
| `crates/xmtp_api_d14n/src/middleware/auth.rs` | `middleware::auth::tests::test_auth_middleware_with_callback_and_handle` | custom async | `API-REQ-025` |
| `crates/xmtp_api_d14n/src/middleware/auth.rs` | `middleware::auth::tests::test_auth_middleware_with_callback_and_handle_concurrent_requests` | custom async | `API-REQ-025` |
| `crates/xmtp_api_d14n/src/protocol/macros.rs` | `doctest:`delegate_envelope_visitor!`example` | Rust doctest | `API-REQ-028` |
| `crates/xmtp_api_d14n/src/protocol/traits/visitor.rs` | `doctest:`EnvelopeVisitor`single-visitor example` | Rust doctest | `API-REQ-028` |
| `crates/xmtp_api_d14n/src/protocol/traits/visitor.rs` | `doctest:`EnvelopeVisitor`multiple-visitors example` | Rust doctest | `API-REQ-028` |
| `crates/xmtp_api_d14n/src/protocol/extractors/aggregate.rs` | `protocol::extractors::aggregate::tests::test_collection_extractor_single_envelope` | custom sync | `API-REQ-030` |
| `crates/xmtp_api_d14n/src/protocol/extractors/aggregate.rs` | `protocol::extractors::aggregate::tests::test_collection_extractor_multiple_envelopes` | custom sync | `API-REQ-030` |
| `crates/xmtp_api_d14n/src/protocol/extractors/aggregate.rs` | `protocol::extractors::aggregate::tests::test_collection_extractor_empty` | custom sync | `API-REQ-030` |
| `crates/xmtp_api_d14n/src/protocol/extractors/aggregate.rs` | `protocol::extractors::aggregate::tests::test_sequenced_extractor_single_envelope` | custom sync | `API-REQ-030` |
| `crates/xmtp_api_d14n/src/protocol/extractors/aggregate.rs` | `protocol::extractors::aggregate::tests::test_sequenced_extractor_multiple_envelopes` | custom sync | `API-REQ-030` |
| `crates/xmtp_api_d14n/src/protocol/extractors/aggregate.rs` | `protocol::extractors::aggregate::tests::test_sequenced_extractor_with_topic_extractor` | custom sync | `API-REQ-030` |
| `crates/xmtp_api_d14n/src/protocol/extractors/aggregate.rs` | `protocol::extractors::aggregate::tests::test_sequenced_extractor_empty` | custom sync | `API-REQ-030` |
| `crates/xmtp_api_d14n/src/protocol/extractors/group_messages.rs` | `protocol::extractors::group_messages::tests::test_extract_group_message_fails_with_mock_data` | custom sync | `API-REQ-032` |
| `crates/xmtp_api_d14n/src/protocol/extractors/identity_updates.rs` | `protocol::extractors::identity_updates::tests::test_extract_identity_update` | custom sync | `API-REQ-032` |
| `crates/xmtp_api_d14n/src/protocol/extractors/key_packages.rs` | `protocol::extractors::key_packages::tests::test_extract_kp` | custom sync | `API-REQ-032` |
| `crates/xmtp_api_d14n/src/protocol/extractors/key_packages.rs` | `protocol::extractors::key_packages::tests::extractor_errors_when_missing` | custom sync | `API-REQ-032` |
| `crates/xmtp_api_d14n/src/protocol/extractors/payloads.rs` | `protocol::extractors::payloads::tests::test_extract_group_message_payload` | custom sync | `API-REQ-032` |
| `crates/xmtp_api_d14n/src/protocol/extractors/payloads.rs` | `protocol::extractors::payloads::tests::test_extract_welcome_message_payload` | custom sync | `API-REQ-032` |
| `crates/xmtp_api_d14n/src/protocol/extractors/payloads.rs` | `protocol::extractors::payloads::tests::test_extract_welcome_pointer_payload` | custom sync | `API-REQ-032` |
| `crates/xmtp_api_d14n/src/protocol/extractors/payloads.rs` | `protocol::extractors::payloads::tests::test_extract_key_package_payload` | custom sync | `API-REQ-032` |
| `crates/xmtp_api_d14n/src/protocol/extractors/payloads.rs` | `protocol::extractors::payloads::tests::test_extract_identity_update_payload` | custom sync | `API-REQ-032` |
| `crates/xmtp_api_d14n/src/protocol/extractors/payloads.rs` | `protocol::extractors::payloads::tests::test_extract_no_payload_fails` | custom sync | `API-REQ-032` |
| `crates/xmtp_api_d14n/src/protocol/extractors/topics.rs` | `protocol::extractors::topics::tests::test_extract_group_message_topic` | custom sync | `API-REQ-032` |
| `crates/xmtp_api_d14n/src/protocol/extractors/topics.rs` | `protocol::extractors::topics::tests::test_extract_welcome_message_topic` | custom sync | `API-REQ-032` |
| `crates/xmtp_api_d14n/src/protocol/extractors/topics.rs` | `protocol::extractors::topics::tests::test_extract_v3_group_message_topic` | custom sync | `API-REQ-032` |
| `crates/xmtp_api_d14n/src/protocol/extractors/topics.rs` | `protocol::extractors::topics::tests::test_extract_v3_welcome_message_topic` | custom sync | `API-REQ-032` |
| `crates/xmtp_api_d14n/src/protocol/extractors/topics.rs` | `protocol::extractors::topics::tests::test_extract_v3_welcome_pointer_topic` | custom sync | `API-REQ-032` |
| `crates/xmtp_api_d14n/src/protocol/extractors/topics.rs` | `protocol::extractors::topics::tests::test_extract_key_package_topic` | custom sync | `API-REQ-032` |
| `crates/xmtp_api_d14n/src/protocol/extractors/topics.rs` | `protocol::extractors::topics::tests::test_extract_identity_update_topic` | custom sync | `API-REQ-032` |
| `crates/xmtp_api_d14n/src/protocol/extractors/topics.rs` | `protocol::extractors::topics::tests::test_extract_missing_key_package_fails` | custom sync | `API-REQ-032` |
| `crates/xmtp_api_d14n/src/protocol/extractors/topics.rs` | `protocol::extractors::topics::tests::test_extract_invalid_hex_identity_fails` | custom sync | `API-REQ-032` |
| `crates/xmtp_api_d14n/src/protocol/extractors/topics.rs` | `protocol::extractors::topics::tests::test_extract_no_topic_fails` | custom sync | `API-REQ-032` |
| `crates/xmtp_api_d14n/src/protocol/extractors/topics.rs` | `protocol::extractors::topics::tests::test_extraction_from_identity_update_req` | custom sync | `API-REQ-032` |
| `crates/xmtp_api_d14n/src/protocol/extractors/welcomes.rs` | `protocol::extractors::welcomes::tests::test_extract_welcome_message` | custom sync | `API-REQ-032` |
| `crates/xmtp_api_d14n/src/protocol/impls/protocol_envelopes.rs` | `protocol::impls::protocol_envelopes::tests::envelope_visitor_flows` | rstest + custom async; cases: group, welcome, key package, identity | `API-REQ-028` |
| `crates/xmtp_api_d14n/src/protocol/impls/protocol_envelopes.rs` | `protocol::impls::protocol_envelopes::tests::envelope_collections` | custom sync | `API-REQ-028` |
| `crates/xmtp_api_d14n/src/protocol/impls/protocol_envelopes.rs` | `protocol::impls::protocol_envelopes::tests::envelope_error_handling` | custom sync | `API-REQ-028` |
| `crates/xmtp_api_d14n/src/protocol/impls/protocol_envelopes.rs` | `protocol::impls::protocol_envelopes::tests::envelope_edge_cases` | custom sync | `API-REQ-028` |
| `crates/xmtp_api_d14n/src/protocol/impls/protocol_envelopes.rs` | `protocol::impls::protocol_envelopes::tests::test_v3_message_visitors` | custom sync; six in-body cases | `API-REQ-028` |
| `crates/xmtp_api_d14n/src/protocol/in_memory_cursor_store.rs` | `protocol::in_memory_cursor_store::tests::test_processed_and_get_latest` | custom sync | `API-REQ-038` |
| `crates/xmtp_api_d14n/src/protocol/in_memory_cursor_store.rs` | `protocol::in_memory_cursor_store::tests::test_merge_on_processed` | custom sync | `API-REQ-038` |
| `crates/xmtp_api_d14n/src/protocol/in_memory_cursor_store.rs` | `protocol::in_memory_cursor_store::tests::test_get_latest_nonexistent_topic` | custom sync | `API-REQ-038` |
| `crates/xmtp_api_d14n/src/protocol/in_memory_cursor_store.rs` | `protocol::in_memory_cursor_store::tests::test_independent_topics` | custom sync | `API-REQ-038` |
| `crates/xmtp_api_d14n/src/protocol/in_memory_cursor_store.rs` | `protocol::in_memory_cursor_store::tests::test_merge_into_empty_store_creates_topic` | custom sync | `API-REQ-038` |
| `crates/xmtp_api_d14n/src/protocol/order.rs` | `protocol::order::test::orders_with_unresolvable_dependencies` | proptest declaration + custom sync | `API-REQ-040` |
| `crates/xmtp_api_d14n/src/protocol/order.rs` | `protocol::order::test::orders_with_recovered_children` | proptest declaration + custom sync | `API-REQ-040` |
| `crates/xmtp_api_d14n/src/protocol/resolve/network_backoff.rs` | `protocol::resolve::network_backoff::tests::test_resolve_all_found_immediately` | custom async | `API-REQ-041` |
| `crates/xmtp_api_d14n/src/protocol/resolve/network_backoff.rs` | `protocol::resolve::network_backoff::tests::test_resolve_partial_resolution` | custom async | `API-REQ-041` |
| `crates/xmtp_api_d14n/src/protocol/resolve/network_backoff.rs` | `protocol::resolve::network_backoff::tests::test_resolve_empty_missing_set` | custom async | `API-REQ-041` |
| `crates/xmtp_api_d14n/src/protocol/sort/causal.rs` | `protocol::sort::causal::tests::causal_sort` | proptest declaration + custom sync | `API-REQ-039` |
| `crates/xmtp_api_d14n/src/protocol/sort/causal.rs` | `protocol::sort::causal::tests::reapplies_within_array` | proptest declaration + custom sync | `API-REQ-039` |
| `crates/xmtp_api_d14n/src/protocol/sort/timestamp.rs` | `protocol::sort::timestamp::tests::sorts_by_timestamp` | custom sync containing proptest | `API-REQ-042` |
| `crates/xmtp_api_d14n/src/queries/combined/tests.rs` | `queries::combined::tests::regex_does_not_panic` | custom sync | `API-REQ-043` |
| `crates/xmtp_api_d14n/src/queries/combined/tests.rs` | `queries::combined::tests::regex_matches_publishing_error` | custom sync | `API-REQ-043` |
| `crates/xmtp_api_d14n/src/queries/combined/tests.rs` | `queries::combined::tests::regex_matches_streaming_error` | custom sync | `API-REQ-043` |
| `crates/xmtp_api_d14n/src/queries/combined/tests.rs` | `queries::combined::tests::choose_client_returns_d14n_when_already_migrated` | custom async; `unwrap_try` | `API-REQ-044` |
| `crates/xmtp_api_d14n/src/queries/combined/tests.rs` | `queries::combined::tests::choose_client_returns_v3_before_cutover` | custom async; `unwrap_try` | `API-REQ-044` |
| `crates/xmtp_api_d14n/src/queries/combined/tests.rs` | `queries::combined::tests::choose_client_returns_d14n_after_cutover` | custom async; `unwrap_try` | `API-REQ-044` |
| `crates/xmtp_api_d14n/src/queries/combined/tests.rs` | `queries::combined::tests::choose_client_refreshes_after_timeout` | custom async; `unwrap_try` | `API-REQ-044` |
| `crates/xmtp_api_d14n/src/queries/combined/tests.rs` | `queries::combined::tests::refresh_cutover_updates_store` | custom async; `unwrap_try` | `API-REQ-044` |
| `crates/xmtp_api_d14n/src/queries/combined/tests.rs` | `queries::combined::tests::write_with_refresh_succeeds_without_retry` | custom async; `unwrap_try` | `API-REQ-044` |
| `crates/xmtp_api_d14n/src/queries/combined/tests.rs` | `queries::combined::tests::write_with_refresh_retries_on_migration_error` | custom async; `unwrap_try` | `API-REQ-044` |
| `crates/xmtp_api_d14n/src/queries/combined/tests.rs` | `queries::combined::tests::write_with_refresh_does_not_retry_on_other_error` | custom async; `unwrap_try` | `API-REQ-044` |
| `crates/xmtp_api_d14n/src/queries/combined/tests.rs` | `queries::combined::tests::is_d14n_returns_false_before_migration` | custom async; `unwrap_try` | `API-REQ-044` |
| `crates/xmtp_api_d14n/src/queries/combined/tests.rs` | `queries::combined::tests::is_d14n_returns_true_after_migration` | custom async; `unwrap_try` | `API-REQ-044` |
| `crates/xmtp_api_d14n/src/queries/combined/tests.rs` | `queries::combined::tests::commit_log_stays_on_v3_after_migration` | custom async; `unwrap_try` | `API-REQ-044` |
| `crates/xmtp_api_d14n/src/queries/d14n/connection.rs` | `queries::d14n::connection::tests::classifies_control_frames` | custom sync; `unwrap_try`; native-only module | `API-REQ-048` |
| `crates/xmtp_api_d14n/src/queries/d14n/connection.rs` | `queries::d14n::connection::tests::parses_topics_live_and_skips_malformed` | custom sync; `unwrap_try`; native-only module | `API-REQ-048` |
| `crates/xmtp_api_d14n/src/queries/d14n/connection.rs` | `queries::d14n::connection::tests::skips_unknown_response_version` | custom sync; `unwrap_try`; native-only module | `API-REQ-048` |
| `crates/xmtp_api_d14n/src/queries/d14n/connection.rs` | `queries::d14n::connection::tests::empty_envelope_batch_yields_empty_messages` | custom sync; `unwrap_try`; native-only module | `API-REQ-048` |
| `crates/xmtp_api_d14n/src/queries/d14n/connection.rs` | `queries::d14n::connection::tests::bad_payload_is_skipped_without_dropping_the_batch` | custom sync; `unwrap_try`; native-only module | `API-REQ-048` |
| `crates/xmtp_api_d14n/src/queries/d14n/connection.rs` | `queries::d14n::connection::tests::malformed_envelope_is_skipped_without_dropping_the_batch` | custom sync; `unwrap_try`; native-only module | `API-REQ-048` |
| `crates/xmtp_api_d14n/src/queries/d14n/identity.rs` | `queries::d14n::identity::tests::publish_identity_update_returns_cursor` | custom async; `unwrap_try`; type annotation plus missing-update error only; no successful cursor case. | `API-REQ-047` |
| `crates/xmtp_api_d14n/src/queries/d14n/mls.rs` | `queries::d14n::mls::tests::test_group_message_response_extractor_with_empty_envelope` | custom sync | `API-REQ-049` |
| `crates/xmtp_api_d14n/src/queries/d14n/mls.rs` | `queries::d14n::mls::tests::test_group_message_response_extractor_builder_pattern` | custom sync | `API-REQ-049` |
| `crates/xmtp_api_d14n/src/queries/d14n/mls.rs` | `queries::d14n::mls::tests::test_send_group_messages_with_dependencies` | proptest declaration + custom sync | `API-REQ-050` |
| `crates/xmtp_api_d14n/src/queries/stream/extractor.rs` | `queries::stream::extractor::test::test_content_scenarios` | rstest + custom async; six cases | `API-REQ-051` |
| `crates/xmtp_api_d14n/src/queries/stream/extractor.rs` | `queries::stream::extractor::test::test_stream_error_propagation` | custom async | `API-REQ-051` |
| `crates/xmtp_api_d14n/src/queries/stream/extractor.rs` | `queries::stream::extractor::test::test_extraction_error_propagation` | custom async | `API-REQ-051` |
| `crates/xmtp_api_d14n/src/queries/stream/extractor.rs` | `queries::stream::extractor::test::stream_can_finish` | custom sync | `API-REQ-051` |
| `crates/xmtp_api_d14n/src/queries/stream/ordered.rs` | `queries::stream::ordered::test::orders_stream_and_ices_missing` | proptest declaration + custom sync | `API-REQ-052` |
| `crates/xmtp_api_d14n/src/queries/stream/status_aware.rs` | `queries::stream::status_aware::tests::test_initial_state` | custom async; `unwrap_try` | `API-REQ-053` |
| `crates/xmtp_api_d14n/src/queries/stream/status_aware.rs` | `queries::stream::status_aware::tests::test_yields_envelopes_from_envelope_response` | custom async; `unwrap_try` | `API-REQ-053` |
| `crates/xmtp_api_d14n/src/queries/stream/status_aware.rs` | `queries::stream::status_aware::tests::test_skips_non_envelope_responses` | custom async; `unwrap_try` | `API-REQ-053` |
| `crates/xmtp_api_d14n/src/queries/stream/status_aware.rs` | `queries::stream::status_aware::tests::test_last_ping_updated` | rstest + custom async; cases: envelope/status/none | `API-REQ-053` |
| `crates/xmtp_api_d14n/src/queries/stream/status_aware.rs` | `queries::stream::status_aware::tests::test_status_flag_set_independently` | rstest + custom async; cases: started/catchup | `API-REQ-053` |
| `crates/xmtp_api_d14n/src/queries/stream/status_aware.rs` | `queries::stream::status_aware::tests::test_full_lifecycle` | custom async; `unwrap_try` | `API-REQ-053` |
| `crates/xmtp_api_d14n/src/queries/client_bundle.rs` | `queries::client_bundle::tests::env_cannot_be_overridden_by_none` | custom sync | `API-REQ-054` |
| `crates/xmtp_api_d14n/src/queries/bidi.rs` | `queries::bidi::tests::probe_nonces_never_mint_the_watchdog_nonce` | custom sync; native-only | `API-REQ-056` |
| `crates/xmtp_api_d14n/src/queries/bidi.rs` | `queries::bidi::tests::drain_after_finish_flushes_pending_before_closing` | custom async; `unwrap_try`; native-only | `API-REQ-056` |
| `crates/xmtp_api_d14n/src/queries/bidi.rs` | `queries::bidi::tests::drain_after_finish_bounds_the_flush_on_a_wedged_wire` | custom async; `unwrap_try`; native-only | `API-REQ-056` |
| `crates/xmtp_api_d14n/src/queries/bidi.rs` | `queries::bidi::tests::watchdog_probes_then_tears_down_a_silent_wire` | custom async; `unwrap_try`; native-only | `API-REQ-057` |
| `crates/xmtp_api_d14n/src/queries/bidi.rs` | `queries::bidi::tests::an_answered_watchdog_probe_keeps_the_wire_alive` | custom async; `unwrap_try`; native-only | `API-REQ-057` |
| `crates/xmtp_api_d14n/src/queries/bidi.rs` | `queries::bidi::tests::inbound_activity_resets_the_watchdog` | custom async; `unwrap_try`; native-only | `API-REQ-057` |
| `crates/xmtp_api_d14n/src/queries/bidi.rs` | `queries::bidi::tests::consumer_backpressure_is_not_wire_silence` | custom async; `unwrap_try`; native-only | `API-REQ-057` |
| `crates/xmtp_api_d14n/src/queries/v3/bidi.rs` | `queries::v3::bidi::tests::encodes_outbound_and_decodes_inbound` | custom async; `unwrap_try`; native-only | `API-REQ-055` |
| `crates/xmtp_api_d14n/src/queries/v3/bidi.rs` | `queries::v3::bidi::tests::tags_open_error_with_subscribe_endpoint` | custom async; `unwrap_try`; native-only | `API-REQ-055` |
| `crates/xmtp_api_d14n/src/queries/v3/connection.rs` | `queries::v3::connection::tests::open_sends_initial_mutate_and_emits_started` | custom async; `unwrap_try`; native-only | `API-REQ-058` |
| `crates/xmtp_api_d14n/src/queries/v3/connection.rs` | `queries::v3::connection::tests::auto_pongs_server_ping_without_surfacing_it` | custom async; `unwrap_try`; native-only | `API-REQ-058` |
| `crates/xmtp_api_d14n/src/queries/v3/connection.rs` | `queries::v3::connection::tests::probe_round_trips_and_pong_is_not_an_event` | custom async; `unwrap_try`; native-only | `API-REQ-058` |
| `crates/xmtp_api_d14n/src/queries/v3/connection.rs` | `queries::v3::connection::tests::mutate_is_forwarded_to_the_wire` | custom async; `unwrap_try`; native-only | `API-REQ-058` |
| `crates/xmtp_api_d14n/src/queries/v3/connection.rs` | `queries::v3::connection::tests::skips_unknown_version_frames_and_survives` | custom async; `unwrap_try`; native-only | `API-REQ-058` |
| `crates/xmtp_api_d14n/src/queries/v3/connection.rs` | `queries::v3::connection::tests::preserves_wire_order_of_history_markers_and_live` | custom async; `unwrap_try`; native-only | `API-REQ-058` |
| `crates/xmtp_api_d14n/src/queries/v3/connection.rs` | `queries::v3::connection::tests::inbound_error_closes_the_connection` | custom async; `unwrap_try`; native-only | `API-REQ-059` |
| `crates/xmtp_api_d14n/src/queries/v3/connection.rs` | `queries::v3::connection::tests::closing_inbound_tears_down_sends` | custom async; `unwrap_try`; native-only | `API-REQ-059` |
| `crates/xmtp_api_d14n/src/queries/v3/connection.rs` | `queries::v3::connection::tests::concurrent_mutate_and_probe_both_reach_the_wire` | custom async; `unwrap_try`; native-only | `API-REQ-058`, `API-REQ-059` |
| `crates/xmtp_api_d14n/src/queries/v3/connection.rs` | `queries::v3::connection::tests::probe_within_times_out_when_no_pong` | custom async; `unwrap_try`; native-only | `API-REQ-059` |
| `crates/xmtp_api_d14n/src/queries/v3/connection.rs` | `queries::v3::connection::tests::default_probe_timeout_tracks_server_keepalive` | custom async; `unwrap_try`; native-only | `API-REQ-059` |
| `crates/xmtp_api_d14n/src/queries/v3/connection.rs` | `queries::v3::connection::tests::busy_wire_does_not_stall_inbound` | custom async; `unwrap_try`; native-only | `API-REQ-059` |
| `crates/xmtp_api_d14n/src/queries/v3/connection.rs` | `queries::v3::connection::tests::gives_up_when_wire_wedged_past_backlog_cap` | custom async; `unwrap_try`; native-only | `API-REQ-059` |
| `crates/xmtp_api_d14n/src/queries/v3/connection.rs` | `queries::v3::connection::tests::finish_is_processed_under_wire_backpressure` | custom async; `unwrap_try`; native-only | `API-REQ-059` |
| `crates/xmtp_api_d14n/src/queries/v3/connection.rs` | `queries::v3::connection::tests::mutate_and_probe_report_closed_after_finish` | custom async; `unwrap_try`; native-only | `API-REQ-059` |
| `crates/xmtp_api_d14n/src/queries/v3/connection.rs` | `queries::v3::connection::tests::finish_resolves_in_flight_probe_to_closed` | custom async; `unwrap_try`; native-only | `API-REQ-059` |
| `crates/xmtp_api_d14n/src/queries/v3/connection.rs` | `queries::v3::connection::tests::try_mutate_reports_full_and_recovers_after_drain` | custom async; `unwrap_try`; native-only | `API-REQ-059` |
| `crates/xmtp_api_d14n/src/queries/v3/connection.rs` | `queries::v3::connection::tests::try_mutate_reports_closed_after_finish` | custom async; `unwrap_try`; native-only | `API-REQ-059` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::first_lease_opens_the_wire_with_its_cursored_adds` | native custom async; `unwrap_try` | `API-REQ-060` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::a_lease_over_the_cap_splits_into_bounded_frames` | native custom async; `unwrap_try` | `API-REQ-061` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::a_reconnect_resume_over_the_cap_splits_into_bounded_frames` | native custom async; `unwrap_try` | `API-REQ-061` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::an_over_cap_lease_still_catching_up_survives_a_wire_death` | native custom async; `unwrap_try` | `API-REQ-061`, `API-REQ-070` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::an_over_cap_lease_survives_suspend_and_resume` | native custom async; `unwrap_try` | `API-REQ-061`, `API-REQ-067` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::a_lease_over_the_byte_budget_splits_into_bounded_frames` | native custom async; `unwrap_try` | `API-REQ-061` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::a_mass_unsubscribe_chunks_the_removes_wave` | native custom async; `unwrap_try` | `API-REQ-061` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::second_lease_is_a_cursored_re_add_on_the_open_wire` | native custom async; `unwrap_try` | `API-REQ-062` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::deliveries_demux_by_topic` | native custom async; `unwrap_try` | `API-REQ-063` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::a_siblings_replay_fills_the_gap_without_repeating_history` | native custom async; `unwrap_try` | `API-REQ-064` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::rotation_ordered_replay_delivers_every_topic` | native custom async; `unwrap_try` | `API-REQ-064` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::tagged_replay_below_last_seen_is_the_owners_alone` | native custom async; `unwrap_try` | `API-REQ-064` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::covered_live_frame_is_dropped` | native custom async; `unwrap_try` | `API-REQ-064` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::shared_topic_fans_out_to_every_lease` | native custom async; `unwrap_try` | `API-REQ-063` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::markers_route_to_their_owners` | native custom async; `unwrap_try` | `API-REQ-063` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::deref_is_refcounted_and_last_lease_closes_the_wire` | native custom async; `unwrap_try` | `API-REQ-065` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::slow_lease_is_dropped_without_blocking_siblings` | native custom async; `unwrap_try` | `API-REQ-065` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::wire_death_reconnects_from_last_seen_positions` | native custom async; `unwrap_try` | `API-REQ-066` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::command_traffic_does_not_postpone_the_reconnect` | native custom async; `unwrap_try` | `API-REQ-066` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::a_caught_up_lease_hears_no_second_topics_live` | native custom async; `unwrap_try` | `API-REQ-066` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::half_open_wire_is_reaped_and_reconnected` | native custom async; `unwrap_try` | `API-REQ-066` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::suspend_half_closes_and_resume_completes_at_catch_up` | native custom async; `unwrap_try` | `API-REQ-067` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::wire_session_span_closes_with_a_reason_on_every_release_path` | native custom async; current-thread flavor; `unwrap_try` | `API-REQ-066` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::concurrent_resumes_join_one_catch_up_wave` | native custom async; `unwrap_try` | `API-REQ-068` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::suspended_transport_stays_off_the_network` | native custom async; `unwrap_try` | `API-REQ-067` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::a_born_suspended_transport_parks_the_first_lease` | native custom async; `unwrap_try` | `API-REQ-068` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::dropping_the_last_lease_settles_resume_waiters` | native custom async; `unwrap_try` | `API-REQ-068` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::suspend_preempts_a_stuck_dial` | native custom async; `unwrap_try` | `API-REQ-068` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::a_preempting_suspend_outranks_a_deferred_resume` | native custom async; `unwrap_try` | `API-REQ-068` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::a_resume_burst_during_an_outage_dials_once` | native custom async; `unwrap_try` | `API-REQ-069` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::resume_with_nothing_to_do_resolves_immediately` | native custom async; `unwrap_try` | `API-REQ-069` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::interrupted_catch_up_is_re_owed_by_a_reissued_wave` | native custom async; `unwrap_try` | `API-REQ-070` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::interrupted_wave_reissues_past_its_progress_on_reconnect` | native custom async; `unwrap_try` | `API-REQ-070` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::resume_without_a_resume_wave_settles_when_reissues_complete` | native custom async; `unwrap_try` | `API-REQ-070` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::replay_does_not_repeat_the_pre_mutate_live_window` | native custom async; `unwrap_try` | `API-REQ-071` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::reissued_replay_skips_frames_the_owner_saw_live` | native custom async; `unwrap_try` | `API-REQ-071` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::shared_topic_reconnect_replays_a_caught_up_siblings_outage_gap` | native custom async; `unwrap_try` | `API-REQ-072` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::reissue_clamps_per_kind_progress_to_each_topics_own_position` | native custom async; `unwrap_try` | `API-REQ-072` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::fully_yanked_wave_defers_catch_up_until_the_claiming_wave_resolves` | native custom async; `unwrap_try` | `API-REQ-073` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::yank_defers_even_when_the_wire_position_trails_the_floor` | native custom async; `unwrap_try` | `API-REQ-073` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::reconnect_folds_a_caught_up_holders_floor_when_nothing_was_delivered` | native custom async; `unwrap_try` | `API-REQ-072` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::lease_during_a_dead_wire_rides_the_resume_open` | native custom async; `unwrap_try` | `API-REQ-074` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::deref_during_a_dead_wire_keeps_the_topic_off_the_reconnect` | native custom async; `unwrap_try` | `API-REQ-074` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::deref_purges_only_the_dropped_leases_unsent_waves` | native custom async; `unwrap_try` | `API-REQ-074` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::empty_lease_is_refused_without_opening_the_wire` | native custom async; `unwrap_try` | `API-REQ-075` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::open_failure_surfaces_and_registers_nothing` | native custom async; `unwrap_try` | `API-REQ-075` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::unretryable_reconnect_closes_every_lease` | native custom async; `unwrap_try` | `API-REQ-075` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::an_untagged_backends_completion_tombstones_the_transport` | native custom async; `unwrap_try` | `API-REQ-075` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::a_retires_removes_wave_is_acked_without_tombstoning` | native custom async; `unwrap_try` | `API-REQ-075` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::a_dropped_owners_in_flight_wave_still_serves_the_leases_it_holds` | native custom async; `unwrap_try` | `API-REQ-073` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::yank_chains_re_park_deferred_completions_hop_by_hop` | native custom async; `unwrap_try` | `API-REQ-073` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::overlapping_waves_on_a_virgin_topic_lose_nothing` | native custom async; `unwrap_try` | `API-REQ-076` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::a_virgin_topics_interrupted_catch_up_resumes_from_its_floor` | native custom async; `unwrap_try` | `API-REQ-076` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::a_withheld_frame_overflow_drops_the_lease_for_recovery` | native custom async; `unwrap_try` | `API-REQ-077` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::flush_replays_withheld_frames_per_kind_in_arrival_order` | native custom async; `unwrap_try` | `API-REQ-077` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | `queries::bidi_transport::tests::an_alternating_withheld_window_flushes_within_the_channel_bound` | native custom async; `unwrap_try` | `API-REQ-077` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport_props.rs` | `queries::bidi_transport_props::ledger_delivers_exactly_the_asked_suffix_in_order` | native proptest; default 32 cases; custom sync | `API-REQ-078` |
| `crates/xmtp_api_d14n/src/queries/bidi_transport_props.rs` | `queries::bidi_transport_props::chunked_ledger_delivers_exactly_the_asked_suffix_in_order` | native proptest; cap cases 1–2; default 32 schedules | `API-REQ-078` |
| `crates/xmtp_api_grpc/src/grpc_client/client.rs` | `grpc_client::client::tests::metadata_test` | custom async | `API-REQ-079` |
| `crates/xmtp_api_grpc/src/grpc_client/native.rs` | `grpc_client::native::keepalive_tests::defaults_when_env_absent` | built-in sync; native-only module | `API-REQ-080` |
| `crates/xmtp_api_grpc/src/grpc_client/native.rs` | `grpc_client::native::keepalive_tests::env_overrides_are_applied` | built-in sync; native-only module | `API-REQ-080` |
| `crates/xmtp_api_grpc/src/grpc_client/native.rs` | `grpc_client::native::keepalive_tests::zero_tcp_keepalive_disables_it` | built-in sync; native-only module | `API-REQ-080` |
| `crates/xmtp_api_grpc/src/grpc_client/native.rs` | `grpc_client::native::keepalive_tests::invalid_values_fall_back_to_defaults` | built-in sync; native-only module | `API-REQ-080` |
| `crates/xmtp_api_grpc/src/streams/default.rs` | `streams::default::tests::test_successful_message_decoding` | rstest + custom async; empty/single/multiple cases | `API-REQ-081` |
| `crates/xmtp_api_grpc/src/streams/default.rs` | `streams::default::tests::test_error_propagation` | custom async | `API-REQ-081` |
| `crates/xmtp_api_grpc/src/streams/default.rs` | `streams::default::tests::stream_ends` | custom sync | `API-REQ-081` |
| `crates/xmtp_api_grpc/src/streams/multiplexed.rs` | `streams::multiplexed::tests::does_not_starve_s2` | custom sync | `API-REQ-082` |
| `crates/xmtp_api_grpc/src/streams/multiplexed.rs` | `streams::multiplexed::tests::polls_s2_in_between_s1` | custom sync | `API-REQ-082` |
| `crates/xmtp_api_grpc/src/streams/multiplexed.rs` | `streams::multiplexed::tests::ignores_items_after_s2_pending` | custom sync | `API-REQ-082` |
| `crates/xmtp_api_grpc/src/streams/multiplexed.rs` | `streams::multiplexed::tests::ends_when_s1_ends` | custom sync | `API-REQ-082` |
| `crates/xmtp_api_grpc/src/streams/multiplexed.rs` | `streams::multiplexed::tests::does_not_panic_on_polling_after_finish` | custom sync | `API-REQ-082` |
| `crates/xmtp_api_grpc/src/streams/non_blocking_stream.rs` | `streams::non_blocking_stream::tests::handles_err_on_establish` | custom sync | `API-REQ-083` |
| `crates/xmtp_api_grpc/src/streams/non_blocking_stream.rs` | `streams::non_blocking_stream::tests::happy_path_future` | custom sync | `API-REQ-083` |
| `crates/xmtp_api_grpc/src/streams/non_blocking_stream.rs` | `streams::non_blocking_stream::tests::establish_changes_state_to_started` | custom sync | `API-REQ-083` |
| `crates/xmtp_api_grpc/src/streams/try_from_item.rs` | `streams::try_from_item::tests::test_successful_conversions` | rstest + custom async; empty/single/3/5-item cases | `API-REQ-084` |
| `crates/xmtp_api_grpc/src/streams/try_from_item.rs` | `streams::try_from_item::tests::test_conversion_error_propagation` | custom async | `API-REQ-084` |
| `crates/xmtp_api_grpc/src/streams/try_from_item.rs` | `streams::try_from_item::tests::stream_can_finish` | custom sync | `API-REQ-084` |
| `crates/xmtp_api_grpc/src/streams/try_from_item.rs` | `streams::try_from_item::tests::happy_path` | custom sync | `API-REQ-084` |
