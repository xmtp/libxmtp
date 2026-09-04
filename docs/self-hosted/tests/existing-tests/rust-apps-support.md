# Rust app and support-crate test index

[← Test inventory](../existing-tests.md) · [Requirements](../existing-requirements.md)

This fragment excludes `apps/android/**`. It records one row per source declaration, so each rstest declaration summarizes its generated cases.

| File | Qualified test | Form / gates / cases | Requirements |
| --- | --- | --- | --- |
| `apps/db_tools/src/tasks/clear_messages.rs` | `tasks::clear_messages::tests::test_clear_msgs_and_groups_still_work` | custom async; unwrap_try | `RUST-REQ-001` |
| `apps/db_tools/src/tasks/clear_messages.rs` | `tasks::clear_messages::tests::test_clear_messages_retention_days` | custom async; unwrap_try | `RUST-REQ-001` |
| `apps/db_tools/src/tasks/db_bench.rs` | `tasks::db_bench::tests::test_bench_works` | custom async; unwrap_try; all returned results must be Ok; no values or timing threshold. | `RUST-REQ-002` |
| `apps/db_tools/src/tasks/group_management.rs` | `tasks::group_management::tests::test_disable_groups` | custom async; unwrap_try | `RUST-REQ-003` |
| `apps/db_tools/src/tasks/migrations.rs` | `tasks::migrations::tests::test_rollback_and_run_pending_migrations` | ignored unit; persistent DB | `RUST-REQ-004` |
| `apps/db_tools/src/tasks/migrations.rs` | `tasks::migrations::tests::test_applied_migrations_returns_versions` | custom async; unwrap_try | `RUST-REQ-004` |
| `apps/db_tools/src/tasks/migrations.rs` | `tasks::migrations::tests::test_run_and_revert_specific_migration` | custom async; unwrap_try | `RUST-REQ-004` |
| `apps/db_tools/src/tasks/migrations.rs` | `tasks::migrations::tests::test_migration_status_applied_and_pending` | custom async; unwrap_try | `RUST-REQ-004` |
| `apps/keepalive-probe/src/main.rs` | `main::tests::percentile_basics` | unit; 4 percentiles | `RUST-REQ-005` |
| `apps/keepalive-probe/src/main.rs` | `main::tests::percentile_empty_is_zero` | unit | `RUST-REQ-005` |
| `apps/keepalive-probe/src/main.rs` | `main::tests::round_secs_rounds_to_nearest` | unit | `RUST-REQ-006` |
| `apps/keepalive-probe/src/main.rs` | `main::tests::classify_buckets` | unit; 3 message cases | `RUST-REQ-007` |
| `apps/mls_validation_service/src/cached_signature_verifier.rs` | `cached_signature_verifier::tests::test_is_valid_signature` | rstest fixture; Tokio; Docker fixture; 60-second timeout | `RUST-REQ-008` |
| `apps/mls_validation_service/src/cached_signature_verifier.rs` | `cached_signature_verifier::tests::test_cache_eviction` | Tokio unit; tests bare third-party `LruCache`, not production wrapper caching. | `RUST-REQ-009` |
| `apps/mls_validation_service/src/cached_signature_verifier.rs` | `cached_signature_verifier::tests::test_cache_key_includes_all_params` | Tokio unit | `RUST-REQ-010` |
| `apps/mls_validation_service/src/cached_signature_verifier.rs` | `cached_signature_verifier::tests::test_missing_verifier` | Tokio unit | `RUST-REQ-008` |
| `apps/mls_validation_service/src/handlers.rs` | `handlers::tests::test_get_association_state` | unit; should panic placeholder; no successful association-state assertion. | `RUST-REQ-012` |
| `apps/mls_validation_service/src/handlers.rs` | `handlers::tests::test_validate_inbox_id_key_package_happy_path` | Tokio unit | `RUST-REQ-013` |
| `apps/mls_validation_service/src/handlers.rs` | `handlers::tests::test_validate_inbox_id_key_package_failure` | Tokio unit | `RUST-REQ-013` |
| `apps/mls_validation_service/src/handlers.rs` | `handlers::tests::test_validate_scw` | rstest fixture; Tokio; Docker fixture; 30-second timeout | `RUST-REQ-014` |
| `apps/mls_validation_service/src/handlers.rs` | `handlers::tests::deserialization_error_maps_to_invalid_argument` | unit | `RUST-REQ-015` |
| `apps/mls_validation_service/src/handlers.rs` | `handlers::tests::retryable_signature_error_maps_to_unavailable` | unit | `RUST-REQ-015` |
| `apps/xmtp_debug/src/app/health/ops/add_members.rs` | `app::health::ops::add_members::tests::names_are_stable` | unit | `RUST-REQ-016` |
| `apps/xmtp_debug/src/app/health/ops/bootstrap.rs` | `app::health::ops::bootstrap::tests::name_is_stable` | unit | `RUST-REQ-016` |
| `apps/xmtp_debug/src/app/health/ops/create_dm.rs` | `app::health::ops::create_dm::tests::name_is_stable` | unit | `RUST-REQ-016` |
| `apps/xmtp_debug/src/app/health/ops/create_group.rs` | `app::health::ops::create_group::tests::name_is_stable` | unit | `RUST-REQ-016` |
| `apps/xmtp_debug/src/app/health/ops/create_identity.rs` | `app::health::ops::create_identity::tests::name_is_stable` | unit | `RUST-REQ-016` |
| `apps/xmtp_debug/src/app/health/ops/get_mutable_metadata.rs` | `app::health::ops::get_mutable_metadata::tests::name_is_stable` | unit | `RUST-REQ-016` |
| `apps/xmtp_debug/src/app/health/ops/leave_group.rs` | `app::health::ops::leave_group::tests::name_is_stable` | unit | `RUST-REQ-016` |
| `apps/xmtp_debug/src/app/health/ops/remove_member.rs` | `app::health::ops::remove_member::tests::name_is_stable` | unit | `RUST-REQ-016` |
| `apps/xmtp_debug/src/app/health/ops/send_message.rs` | `app::health::ops::send_message::tests::name_is_stable` | unit | `RUST-REQ-016` |
| `apps/xmtp_debug/src/app/health/ops/update_admin_list.rs` | `app::health::ops::update_admin_list::tests::name_is_stable` | unit | `RUST-REQ-016` |
| `apps/xmtp_debug/src/app/health/ops/update_app_data.rs` | `app::health::ops::update_app_data::tests::name_is_stable` | unit | `RUST-REQ-016` |
| `apps/xmtp_debug/src/app/health/ops/update_commit_log_signer.rs` | `app::health::ops::update_commit_log_signer::tests::name_is_stable` | unit | `RUST-REQ-016` |
| `apps/xmtp_debug/src/app/health/ops/update_consent_state.rs` | `app::health::ops::update_consent_state::tests::names_are_stable` | unit | `RUST-REQ-016` |
| `apps/xmtp_debug/src/app/health/ops/update_group_description.rs` | `app::health::ops::update_group_description::tests::name_is_stable` | unit | `RUST-REQ-016` |
| `apps/xmtp_debug/src/app/health/ops/update_group_image_url.rs` | `app::health::ops::update_group_image_url::tests::name_is_stable` | unit | `RUST-REQ-016` |
| `apps/xmtp_debug/src/app/health/ops/update_group_name.rs` | `app::health::ops::update_group_name::tests::name_is_stable` | unit | `RUST-REQ-016` |
| `apps/xmtp_debug/src/app/health/ops/update_message_disappearing.rs` | `app::health::ops::update_message_disappearing::tests::names_are_stable` | unit | `RUST-REQ-016` |
| `apps/xmtp_debug/src/app/health/ops/update_permission_policy.rs` | `app::health::ops::update_permission_policy::tests::name_is_stable` | unit | `RUST-REQ-016` |
| `apps/xmtp_debug/src/app/health/ops/upload_key_package.rs` | `app::health::ops::upload_key_package::tests::name_is_stable` | unit | `RUST-REQ-016` |
| `apps/xmtp_debug/src/app/health/registry.rs` | `app::health::registry::tests::empty_input_returns_empty` | unit | `RUST-REQ-017` |
| `apps/xmtp_debug/src/app/health/registry.rs` | `app::health::registry::tests::single_root_no_deps` | unit | `RUST-REQ-017` |
| `apps/xmtp_debug/src/app/health/registry.rs` | `app::health::registry::tests::linear_chain_orders_by_dependency` | unit | `RUST-REQ-017` |
| `apps/xmtp_debug/src/app/health/registry.rs` | `app::health::registry::tests::diamond_dag_orders_correctly` | unit | `RUST-REQ-017` |
| `apps/xmtp_debug/src/app/health/registry.rs` | `app::health::registry::tests::ties_break_alphabetically` | unit | `RUST-REQ-017` |
| `apps/xmtp_debug/src/app/health/registry.rs` | `app::health::registry::tests::duplicate_name_panics` | unit; should panic | `RUST-REQ-017` |
| `apps/xmtp_debug/src/app/health/registry.rs` | `app::health::registry::tests::unknown_dep_panics` | unit; should panic | `RUST-REQ-017` |
| `apps/xmtp_debug/src/app/health/registry.rs` | `app::health::registry::tests::cycle_panics` | unit; should panic | `RUST-REQ-017` |
| `apps/xmtp_debug/src/app/health/registry.rs` | `app::health::registry::tests::gated_entry_skipped_when_condition_inactive` | unit | `RUST-REQ-017` |
| `apps/xmtp_debug/src/app/health/registry.rs` | `app::health::registry::tests::gated_entry_runs_when_condition_active` | unit | `RUST-REQ-017` |
| `apps/xmtp_debug/src/app/health/result.rs` | `app::health::result::tests::report_counts_and_failures` | unit | `RUST-REQ-020` |
| `apps/xmtp_debug/src/app/health/result.rs` | `app::health::result::tests::empty_report_has_no_failures` | unit | `RUST-REQ-020` |
| `apps/xmtp_debug/src/app/health/result.rs` | `app::health::result::tests::skipped_counts_as_skipped_not_pass_or_fail` | unit | `RUST-REQ-020` |
| `apps/xmtp_debug/src/app/health/result.rs` | `app::health::result::tests::skipped_constructor_records_missing_conditions` | unit | `RUST-REQ-020` |
| `apps/xmtp_debug/src/app/health/validators/no_forks.rs` | `app::health::validators::no_forks::tests::name_is_stable` | unit | `RUST-REQ-016` |
| `apps/xmtp_debug/src/app/health/validators/no_missing_messages.rs` | `app::health::validators::no_missing_messages::tests::name_is_stable` | unit | `RUST-REQ-016` |
| `apps/xmtp_debug/src/app/store/messages.rs` | `app::store::messages::tests::message_store_set_then_get_roundtrips` | unit | `RUST-REQ-023` |
| `apps/xmtp_debug/src/app/store/messages.rs` | `app::store::messages::tests::message_store_load_returns_all_messages_for_network` | unit | `RUST-REQ-023` |
| `apps/xmtp_debug/src/app/store/messages.rs` | `app::store::messages::tests::message_store_load_then_filter_by_group_id` | unit | `RUST-REQ-023` |
| `apps/xmtp_debug/src/app/store.rs` | `app::store::identity_key_tests::roundtrips_through_redb_value` | unit | `RUST-REQ-021` |
| `apps/xmtp_debug/src/app/store.rs` | `app::store::identity_key_tests::compare_orders_by_network_then_version_then_inbox` | table-driven unit; 6 cases | `RUST-REQ-021` |
| `apps/xmtp_debug/src/app/store.rs` | `app::store::identity_database_tests::set_then_load_returns_all_versions_for_network` | unit | `RUST-REQ-022` |
| `apps/xmtp_debug/src/args.rs` | `args::tests::perf_with_d14n_and_backend_is_valid` | unit | `RUST-REQ-024` |
| `apps/xmtp_debug/src/args.rs` | `args::tests::explicit_gateway_url_overrides_perf` | unit | `RUST-REQ-024` |
| `apps/xmtp_debug/src/metrics.rs` | `metrics::tests::csv_metrics_toggle_roundtrip` | unit | `RUST-REQ-025` |
| `apps/xmtp_debug/tests/healthcheck.rs` | `healthcheck::healthcheck_passes_on_local_backend` | ignored integration; requires local dev/up | `RUST-REQ-026` |
| `apps/xnet/lib/src/config/address_mode.rs` | `config::address_mode::tests::local_hostname` | unit | `RUST-REQ-027` |
| `apps/xnet/lib/src/config/address_mode.rs` | `config::address_mode::tests::dns_domain` | unit | `RUST-REQ-027` |
| `apps/xnet/lib/src/config/address_mode.rs` | `config::address_mode::tests::is_remote` | unit | `RUST-REQ-027` |
| `apps/xnet/lib/src/config/address_mode.rs` | `config::address_mode::tests::remote_domain_hostname` | unit | `RUST-REQ-027` |
| `apps/xnet/lib/src/config/toml_config_test.rs` | `config::toml_config_test::paused_defaults_to_false` | unit | `RUST-REQ-028` |
| `apps/xnet/lib/src/config/toml_config_test.rs` | `config::toml_config_test::paused_parses_true` | unit | `RUST-REQ-028` |
| `apps/xnet/lib/src/config/toml_config_test.rs` | `config::toml_config_test::paused_parses_false_explicit` | unit | `RUST-REQ-028` |
| `apps/xnet/lib/src/config/toml_config_test.rs` | `config::toml_config_test::node_use_standard_port_defaults_to_false` | unit | `RUST-REQ-028` |
| `apps/xnet/lib/src/config/toml_config_test.rs` | `config::toml_config_test::node_use_standard_port_parses_true` | unit | `RUST-REQ-028` |
| `apps/xnet/lib/src/config/toml_config_test.rs` | `config::toml_config_test::validation_rejects_two_standard_port_nodes` | unit | `RUST-REQ-028` |
| `apps/xnet/lib/src/config/toml_config_test.rs` | `config::toml_config_test::validation_rejects_standard_port_with_explicit_port` | unit | `RUST-REQ-028` |
| `apps/xnet/lib/src/config/toml_config_test.rs` | `config::toml_config_test::validation_allows_one_standard_port_node` | unit | `RUST-REQ-028` |
| `apps/xnet/lib/src/config/toml_config_test.rs` | `config::toml_config_test::validation_allows_zero_standard_port_nodes` | unit | `RUST-REQ-028` |
| `apps/xnet/lib/src/config/toml_config_test.rs` | `config::toml_config_test::extra_traefik_routes_defaults_to_empty` | unit | `RUST-REQ-028` |
| `apps/xnet/lib/src/config/toml_config_test.rs` | `config::toml_config_test::extra_traefik_routes_parses_single_route` | unit | `RUST-REQ-028` |
| `apps/xnet/lib/src/config/toml_config_test.rs` | `config::toml_config_test::remote_domain_is_valid` | unit | `RUST-REQ-028` |
| `apps/xnet/lib/src/config/toml_config_test.rs` | `config::toml_config_test::no_remote_domain_is_valid` | unit | `RUST-REQ-028` |
| `apps/xnet/lib/src/config/toml_config_test.rs` | `config::toml_config_test::remote_domain_rejects_empty` | unit | `RUST-REQ-028` |
| `apps/xnet/lib/src/config/toml_config_test.rs` | `config::toml_config_test::remote_domain_rejects_leading_dot` | unit | `RUST-REQ-028` |
| `apps/xnet/lib/src/config/toml_config_test.rs` | `config::toml_config_test::remote_domain_rejects_trailing_dot` | unit | `RUST-REQ-028` |
| `apps/xnet/lib/src/config/toml_config_test.rs` | `config::toml_config_test::extra_traefik_routes_parses_multiple_routes` | unit | `RUST-REQ-028` |
| `apps/xnet/lib/src/node_provisioner.rs` | `node_provisioner::tests::derive_signers_index_formula` | table-driven unit; node IDs 100/200/300 | `RUST-REQ-032` |
| `apps/xnet/lib/src/node_provisioner.rs` | `node_provisioner::tests::derive_signers_first_node_indices` | unit | `RUST-REQ-032` |
| `apps/xnet/lib/src/node_provisioner.rs` | `node_provisioner::tests::derive_signers_zero_id` | unit | `RUST-REQ-032` |
| `apps/xnet/lib/src/node_provisioner.rs` | `node_provisioner::tests::derive_signers_max_node_within_bounds` | unit | `RUST-REQ-032` |
| `apps/xnet/lib/src/node_provisioner.rs` | `node_provisioner::tests::derive_signers_overflow_detection` | unit | `RUST-REQ-032` |
| `apps/xnet/lib/src/services/traefik_config.rs` | `services::traefik_config::tests::write_with_no_routes_produces_empty_config` | unit | `RUST-REQ-033` |
| `apps/xnet/lib/src/services/traefik_config.rs` | `services::traefik_config::tests::extra_routes_appear_in_dynamic_yaml` | unit | `RUST-REQ-033` |
| `apps/xnet/lib/src/services/traefik_config.rs` | `services::traefik_config::tests::extra_routes_without_priority_omit_priority_field` | unit | `RUST-REQ-033` |
| `apps/xnet/lib/src/services/traefik_config.rs` | `services::traefik_config::tests::extra_routes_merge_with_auto_routes` | unit | `RUST-REQ-033` |
| `apps/xnet/lib/src/services/traefik_config.rs` | `services::traefik_config::tests::extra_routes_not_lost_after_add_route` | unit | `RUST-REQ-033` |
| `apps/xnet/lib/src/services/traefik_config.rs` | `services::traefik_config::tests::load_from_file_ignores_extra_routes_in_yaml` | unit | `RUST-REQ-033` |
| `apps/xnet/lib/src/services/traefik_config.rs` | `services::traefik_config::tests::routes_listen_on_http_entrypoint` | unit | `RUST-REQ-033` |
| `apps/xnet/lib/src/services/traefik_config.rs` | `services::traefik_config::tests::extra_routes_listen_on_http_entrypoint` | unit | `RUST-REQ-033` |
| `apps/xnet/lib/src/types.rs` | `types::tests::resolve_port_standard_port_none_returns_5050` | unit | `RUST-REQ-036` |
| `apps/xnet/lib/src/types.rs` | `types::tests::resolve_port_auto_allocates_in_range` | unit | `RUST-REQ-036` |
| `apps/xnet/lib/src/types.rs` | `types::tests::resolve_port_explicit_port_returns_that_port` | unit | `RUST-REQ-036` |
| `apps/xnet/lib/src/types.rs` | `types::tests::resolve_port_standard_port_and_explicit_errors` | unit | `RUST-REQ-036` |
| `apps/xnet/lib/src/wallet_funding.rs` | `wallet_funding::tests::test_fund_wallet` | ignored integration; requires Anvil localhost:8545 | `RUST-REQ-037` |
| `crates/xmtp_common/src/error_code.rs` | `error_code::tests::test_struct_error_code` | unit | `RUST-REQ-038` |
| `crates/xmtp_common/src/error_code.rs` | `error_code::tests::test_enum_error_code` | unit | `RUST-REQ-038` |
| `crates/xmtp_common/src/error_code.rs` | `error_code::tests::test_inherited_error_code` | unit | `RUST-REQ-038` |
| `crates/xmtp_common/src/error_code.rs` | `error_code::tests::test_boxed_error_code` | unit | `RUST-REQ-038` |
| `crates/xmtp_common/src/error_code.rs` | `error_code::tests::test_ref_error_code` | unit | `RUST-REQ-038` |
| `crates/xmtp_common/src/error_code.rs` | `error_code::tests::test_custom_error_code` | unit | `RUST-REQ-038` |
| `crates/xmtp_common/src/error_code.rs` | `error_code::tests::test_signature_error_codes` | unit | `RUST-REQ-038` |
| `crates/xmtp_common/src/error_code.rs` | `error_code::tests::test_identifier_validation_error_codes` | unit | `RUST-REQ-038` |
| `crates/xmtp_common/src/error_code.rs` | `error_code::tests::test_ethereum_crypto_error_codes` | unit | `RUST-REQ-038` |
| `crates/xmtp_common/src/error_code.rs` | `error_code::tests::test_hex_from_hex_error_code` | unit | `RUST-REQ-038` |
| `crates/xmtp_common/src/fmt.rs` | `fmt::tests::test_long_hex` | unit | `RUST-REQ-041` |
| `crates/xmtp_common/src/hex.rs` | `hex::tests::test_normalize_hex_str_with_mixed_case_prefix` | table-driven unit; 3 cases | `RUST-REQ-042` |
| `crates/xmtp_common/src/hex.rs` | `hex::tests::test_normalize_hex_str_without_prefix` | table-driven unit; 4 cases | `RUST-REQ-042` |
| `crates/xmtp_common/src/hex.rs` | `hex::tests::test_normalize_hex_str_already_normalized` | table-driven unit; 3 cases | `RUST-REQ-042` |
| `crates/xmtp_common/src/hex.rs` | `hex::tests::test_normalize_hex_str_edge_cases` | table-driven unit; 5 cases | `RUST-REQ-042` |
| `crates/xmtp_common/src/http.rs` | `http::tests::bundled_roots_config_is_accepted_by_reqwest` | custom sync; unwrap_try; native non-wasm | `RUST-REQ-043` |
| `crates/xmtp_common/src/retry.rs` | `retry::tests::it_retries_twice_and_succeeds` | custom async | `RUST-REQ-044` |
| `crates/xmtp_common/src/retry.rs` | `retry::tests::it_works_with_random_args` | custom async | `RUST-REQ-044` |
| `crates/xmtp_common/src/retry.rs` | `retry::tests::it_fails_on_three_retries` | custom async | `RUST-REQ-044` |
| `crates/xmtp_common/src/retry.rs` | `retry::tests::it_only_runs_non_retryable_once` | custom async | `RUST-REQ-044` |
| `crates/xmtp_common/src/retry.rs` | `retry::tests::it_works_async` | custom async | `RUST-REQ-044` |
| `crates/xmtp_common/src/retry.rs` | `retry::tests::it_works_async_mut` | custom async | `RUST-REQ-044` |
| `crates/xmtp_common/src/retry.rs` | `retry::tests::backoff_retry` | custom sync | `RUST-REQ-045` |
| `crates/xmtp_common/src/snippet.rs` | `snippet::tests::test_str_snippet` | unit | `RUST-REQ-046` |
| `crates/xmtp_common/src/snippet.rs` | `snippet::tests::test_bytes_snippet` | unit | `RUST-REQ-046` |
| `crates/xmtp_common/src/snippet.rs` | `snippet::tests::test_string_snippet` | unit | `RUST-REQ-046` |
| `crates/xmtp_common/src/snippet.rs` | `snippet::tests::test_option_snippet` | unit | `RUST-REQ-046` |
| `crates/xmtp_common/src/time.rs` | `time::tests::test_expired_error_code` | unit | `RUST-REQ-047` |
| `crates/xmtp_common/src/time.rs` | `time::tests::test_expired_display` | unit | `RUST-REQ-047` |
| `crates/xmtp_common/src/time.rs` | `time::tests::test_expired_description` | unit | `RUST-REQ-047` |
| `crates/xmtp_common/src/time.rs` | `time::tests::test_now_ns_returns_positive` | unit | `RUST-REQ-048` |
| `crates/xmtp_common/src/time.rs` | `time::tests::test_now_ms_returns_positive` | unit | `RUST-REQ-048` |
| `crates/xmtp_common/src/time.rs` | `time::tests::test_now_secs_returns_positive` | unit | `RUST-REQ-048` |
| `crates/xmtp_common/src/time.rs` | `time::tests::jitter::zero_jitter_uses_base_period` | Tokio unit; native-only; Tokio paused clock | `RUST-REQ-049` |
| `crates/xmtp_common/src/time.rs` | `time::tests::jitter::jitter_keeps_ticks_within_bounds` | Tokio unit; native-only; Tokio paused clock | `RUST-REQ-049` |
| `crates/xmtp_common/tests/hub_binding.rs` | `hub_binding::concurrent_tasks_keep_their_own_breadcrumb_trails` | custom sync; unwrap_try; native-only | `RUST-REQ-050` |
| `crates/xmtp_common/tests/hub_binding.rs` | `hub_binding::err_span_hub_keeps_inner_spans_under_the_ffi_transaction` | custom sync; unwrap_try; native-only | `RUST-REQ-051` |
| `crates/xmtp_common/tests/hub_binding.rs` | `hub_binding::task_hub_bound_before_enable_picks_up_a_late_client` | custom sync; unwrap_try; native-only | `RUST-REQ-050` |
| `crates/xmtp_common/tests/hub_binding.rs` | `hub_binding::task_hub_follows_a_disable_then_re_enable` | custom sync; unwrap_try; native-only | `RUST-REQ-050` |
| `crates/xmtp_common/tests/hub_binding.rs` | `hub_binding::err_span_error_event_carries_the_task_hub_trail` | custom sync; unwrap_try; native-only | `RUST-REQ-051` |
| `crates/xmtp_common/tests/span_fields.rs` | `span_fields::span_macros_emit_sentry_fields` | custom sync; unwrap_try; native-only | `RUST-REQ-052` |
| `crates/xmtp_common/tests/xmtp_macro_integration.rs` | `xmtp_macro_integration::try_test` | custom async | `RUST-REQ-053` |
| `crates/xmtp_common/tests/xmtp_macro_integration.rs` | `xmtp_macro_integration::try_test_sync` | custom sync | `RUST-REQ-053` |
| `crates/xmtp_common/tests/xmtp_macro_integration.rs` | `xmtp_macro_integration::try_test_flavor` | custom async | `RUST-REQ-053` |
| `crates/xmtp_common/tests/xmtp_macro_integration.rs` | `xmtp_macro_integration::try_unwrap_try` | unit; should panic | `RUST-REQ-053` |
| `crates/xmtp_common/tests/xmtp_macro_integration.rs` | `xmtp_macro_integration::try_disable_logging` | custom sync | `RUST-REQ-053` |
| `crates/xmtp_configuration/src/common/env.rs` | `common::env::tests::centralized_envs_have_api_url` | unit | `RUST-REQ-055` |
| `crates/xmtp_configuration/src/common/env.rs` | `common::env::tests::d14n_envs_have_no_api_url` | unit | `RUST-REQ-055` |
| `crates/xmtp_configuration/src/common/env.rs` | `common::env::tests::is_d14n_returns_correct_values` | unit | `RUST-REQ-055` |
| `crates/xmtp_content_types/src/actions.rs` | `actions::tests::encode_decode_actions` | custom sync; unwrap_try | `RUST-REQ-056` |
| `crates/xmtp_content_types/src/actions.rs` | `actions::tests::expires_at_serializes_as_utc_with_millis` | custom sync; unwrap_try | `RUST-REQ-056` |
| `crates/xmtp_content_types/src/attachment.rs` | `attachment::tests::test_encode_decode_attachment` | native unit or wasm-bindgen test | `RUST-REQ-058` |
| `crates/xmtp_content_types/src/compatibility_test.rs` | `compatibility_test::integration_test` | unit; fixture file | `RUST-REQ-060` |
| `crates/xmtp_content_types/src/delete_message.rs` | `delete_message::tests::test_encode_decode` | native unit or wasm-bindgen test | `RUST-REQ-058` |
| `crates/xmtp_content_types/src/encryption.rs` | `encryption::tests::test_encrypt_decrypt_roundtrip` | native unit or wasm-bindgen test | `RUST-REQ-063` |
| `crates/xmtp_content_types/src/encryption.rs` | `encryption::tests::test_decrypt_wrong_secret_fails` | native unit or wasm-bindgen test | `RUST-REQ-063` |
| `crates/xmtp_content_types/src/encryption.rs` | `encryption::tests::test_encrypt_produces_different_output_each_time` | native unit or wasm-bindgen test; probabilistic inequality | `RUST-REQ-063` |
| `crates/xmtp_content_types/src/encryption.rs` | `encryption::tests::test_encrypted_payload_sizes` | native unit or wasm-bindgen test | `RUST-REQ-063` |
| `crates/xmtp_content_types/src/encryption.rs` | `encryption::tests::test_invalid_salt_length` | native unit or wasm-bindgen test | `RUST-REQ-063` |
| `crates/xmtp_content_types/src/encryption.rs` | `encryption::tests::test_invalid_nonce_length` | native unit or wasm-bindgen test | `RUST-REQ-063` |
| `crates/xmtp_content_types/src/group_updated.rs` | `group_updated::tests::test_encode_decode` | native unit or wasm-bindgen test | `RUST-REQ-058` |
| `crates/xmtp_content_types/src/intent.rs` | `intent::tests::encode_decode_intent` | custom sync; unwrap_try | `RUST-REQ-059` |
| `crates/xmtp_content_types/src/leave_request.rs` | `leave_request::tests::test_encode_decode` | native unit or wasm-bindgen test | `RUST-REQ-058` |
| `crates/xmtp_content_types/src/lib.rs` | `tests::test_encoded_content_conversion` | unit | `RUST-REQ-061` |
| `crates/xmtp_content_types/src/markdown.rs` | `markdown::tests::can_encode_and_decode_markdown` | native unit or wasm-bindgen test | `RUST-REQ-059` |
| `crates/xmtp_content_types/src/membership_change.rs` | `membership_change::tests::test_encode_decode` | native unit or wasm-bindgen test | `RUST-REQ-058` |
| `crates/xmtp_content_types/src/multi_remote_attachment.rs` | `multi_remote_attachment::tests::test_encode_decode` | native unit or wasm-bindgen test | `RUST-REQ-058` |
| `crates/xmtp_content_types/src/reaction.rs` | `reaction::tests::test_encode_decode` | native unit or wasm-bindgen test | `RUST-REQ-058` |
| `crates/xmtp_content_types/src/reaction.rs` | `reaction::tests::test_legacy_reaction_deserialization` | native unit or wasm-bindgen test | `RUST-REQ-062` |
| `crates/xmtp_content_types/src/read_receipt.rs` | `read_receipt::tests::test_encode_decode_read_receipt` | native unit or wasm-bindgen test | `RUST-REQ-058` |
| `crates/xmtp_content_types/src/remote_attachment.rs` | `remote_attachment::tests::test_encode_decode_remote_attachment` | native unit or wasm-bindgen test | `RUST-REQ-065` |
| `crates/xmtp_content_types/src/remote_attachment.rs` | `remote_attachment::tests::test_encrypt_decrypt_attachment_roundtrip` | native unit or wasm-bindgen test | `RUST-REQ-065` |
| `crates/xmtp_content_types/src/remote_attachment.rs` | `remote_attachment::tests::test_decrypt_with_wrong_digest_fails` | native unit or wasm-bindgen test | `RUST-REQ-065` |
| `crates/xmtp_content_types/src/remote_attachment.rs` | `remote_attachment::tests::test_decrypt_with_wrong_secret_fails` | native unit or wasm-bindgen test | `RUST-REQ-065` |
| `crates/xmtp_content_types/src/remote_attachment.rs` | `remote_attachment::tests::test_decode_with_invalid_salt_hex` | native unit or wasm-bindgen test | `RUST-REQ-065` |
| `crates/xmtp_content_types/src/remote_attachment.rs` | `remote_attachment::tests::test_decode_with_invalid_nonce_hex` | native unit or wasm-bindgen test | `RUST-REQ-065` |
| `crates/xmtp_content_types/src/remote_attachment.rs` | `remote_attachment::tests::test_decode_with_invalid_secret_hex` | native unit or wasm-bindgen test | `RUST-REQ-065` |
| `crates/xmtp_content_types/src/remote_attachment.rs` | `remote_attachment::tests::test_decode_with_invalid_content_length` | native unit or wasm-bindgen test | `RUST-REQ-065` |
| `crates/xmtp_content_types/src/reply.rs` | `reply::tests::test_encode_decode_reply` | native unit or wasm-bindgen test | `RUST-REQ-059` |
| `crates/xmtp_content_types/src/text.rs` | `text::tests::can_encode_and_decode_text` | native unit or wasm-bindgen test | `RUST-REQ-059` |
| `crates/xmtp_content_types/src/transaction_reference.rs` | `transaction_reference::tests::test_encode_decode_transaction_reference` | native unit or wasm-bindgen test | `RUST-REQ-059` |
| `crates/xmtp_content_types/src/wallet_send_calls.rs` | `wallet_send_calls::tests::test_encode_decode_wallet_send_calls` | native unit or wasm-bindgen test | `RUST-REQ-059` |
| `crates/xmtp_logging/src/builder.rs` | `builder::tests::builder_from_config_sets_fields` | unit | `RUST-REQ-067` |
| `crates/xmtp_logging/src/builder.rs` | `builder::tests::builder_methods_mutate_config` | unit | `RUST-REQ-067` |
| `crates/xmtp_logging/src/builder.rs` | `builder::tests::builder_default_is_info_compact` | unit | `RUST-REQ-067` |
| `crates/xmtp_logging/src/builder.rs` | `builder::tests::native_level_defaults_none_and_is_settable` | unit | `RUST-REQ-067` |
| `crates/xmtp_logging/src/builder.rs` | `builder::tests::stdout_level_defaults_none_and_is_settable` | unit | `RUST-REQ-067` |
| `crates/xmtp_logging/src/builder.rs` | `builder::tests::install_then_set_level` | unit; native-only; single global install | `RUST-REQ-068` |
| `crates/xmtp_logging/src/config.rs` | `config::tests::level_strings` | unit | `RUST-REQ-067` |
| `crates/xmtp_logging/src/config.rs` | `config::tests::file_config_carries_level` | unit | `RUST-REQ-067` |
| `crates/xmtp_logging/src/filter.rs` | `filter::tests::test_filter_correct` | unit; 7 level strings | `RUST-REQ-070` |
| `crates/xmtp_logging/src/filter.rs` | `filter::tests::stdout_filter_at_warn_drops_xmtp_info` | unit | `RUST-REQ-070` |
| `crates/xmtp_logging/src/layers/file.rs` | `layers::file::tests::file_writer_errors_on_unwritable_dir` | unit; native-only | `RUST-REQ-071` |
| `crates/xmtp_logging/src/layers/fmt.rs` | `layers::fmt::tests::plain_text_hides_sentry_fields` | unit | `RUST-REQ-072` |
| `crates/xmtp_logging/src/sentry.rs` | `sentry::tests::stable_id_matches_reference_vectors` | unit; 5 fixed vectors; sentry feature; native-only | `RUST-REQ-073` |
| `crates/xmtp_logging/src/sentry.rs` | `sentry::tests::scrub_value_hashes_only_id_keys` | unit; sentry feature; native-only | `RUST-REQ-073` |
| `crates/xmtp_logging/src/sentry.rs` | `sentry::layer_tests::spans_pass_on_sentry_op_or_severity` | unit; sentry feature; native-only | `RUST-REQ-074` |
| `crates/xmtp_logging/src/sentry.rs` | `sentry::layer_tests::error_events_promote_only_at_ffi_boundary` | unit; sentry feature; native-only | `RUST-REQ-074` |
| `crates/xmtp_logging/src/sentry.rs` | `sentry::layer_tests::breadcrumb_scrub_hashes_ids_and_user_is_stamped` | unit; sentry feature; native-only | `RUST-REQ-075` |
| `crates/xmtp_logging/src/sentry.rs` | `sentry::layer_tests::crypto_provider_is_installed_for_the_transport` | unit; sentry feature; native-only | `RUST-REQ-076` |
| `crates/xmtp_logging/src/sentry.rs` | `sentry::layer_tests::client_builds_a_real_transport_without_a_host_installed_provider` | unit; sentry feature; native-only | `RUST-REQ-076` |
| `crates/xmtp_logging/src/sentry.rs` | `sentry::layer_tests::invalid_dsn_is_an_error` | unit; sentry feature; native-only | `RUST-REQ-077` |
| `crates/xmtp_logging/src/sentry.rs` | `sentry::layer_tests::transactions_carry_config_tags_without_clobbering_their_own` | unit; sentry feature; native-only | `RUST-REQ-078` |
| `crates/xmtp_logging/src/sentry.rs` | `sentry::layer_tests::caller_supplied_component_tag_wins_over_the_default` | unit; sentry feature; native-only | `RUST-REQ-078` |
| `crates/xmtp_logging/src/sentry.rs` | `sentry::layer_tests::envelope_rebuild_preserves_headers_and_leaves_events_alone` | unit; sentry feature; native-only | `RUST-REQ-078` |
| `crates/xmtp_logging/src/sentry.rs` | `sentry::layer_tests::sentry_tracing_event_is_scrubbed_in_every_container` | unit; sentry feature; native-only | `RUST-REQ-079` |
| `crates/xmtp_logging/tests/sentry_slot.rs` | `sentry_slot::sentry_slot_lifecycle` | unit; sentry feature; native-only; ordered globals | `RUST-REQ-080` |
| `crates/xmtp_macro/src/builder_test.rs` | `builder_test::test_to_camel_case` | token-expansion unit | `RUST-REQ-081` |
| `crates/xmtp_macro/src/builder_test.rs` | `builder_test::test_required_field_in_constructor` | token-expansion unit | `RUST-REQ-082` |
| `crates/xmtp_macro/src/builder_test.rs` | `builder_test::test_optional_field_setter` | token-expansion unit | `RUST-REQ-082` |
| `crates/xmtp_macro/src/builder_test.rs` | `builder_test::test_default_field` | token-expansion unit | `RUST-REQ-082` |
| `crates/xmtp_macro/src/builder_test.rs` | `builder_test::test_skip_field` | token-expansion unit | `RUST-REQ-082` |
| `crates/xmtp_macro/src/builder_test.rs` | `builder_test::test_optional_must_be_option_type` | token-expansion unit | `RUST-REQ-082` |
| `crates/xmtp_macro/src/builder_test.rs` | `builder_test::test_missing_builder_attribute` | token-expansion unit | `RUST-REQ-082` |
| `crates/xmtp_macro/src/builder_test.rs` | `builder_test::test_multiple_required_fields` | token-expansion unit | `RUST-REQ-082` |
| `crates/xmtp_macro/src/builder_test.rs` | `builder_test::test_all_field_modes_together` | token-expansion unit | `RUST-REQ-082` |
| `crates/xmtp_macro/src/builder_test.rs` | `builder_test::test_annotations_are_applied` | token-expansion unit | `RUST-REQ-082` |
| `crates/xmtp_macro/src/builder_test.rs` | `builder_test::test_setter_prefix` | token-expansion unit | `RUST-REQ-082` |
| `crates/xmtp_macro/src/builder_test.rs` | `builder_test::test_duplicate_builder_attribute_rejected` | token-expansion unit | `RUST-REQ-082` |
| `crates/xmtp_macro/src/builder_test.rs` | `builder_test::test_non_builder_attrs_preserved` | token-expansion unit | `RUST-REQ-082` |
| `crates/xmtp_macro/src/builder_test.rs` | `builder_test::test_generic_struct_with_bounds` | token-expansion unit | `RUST-REQ-082` |
| `crates/xmtp_macro/src/builder_test.rs` | `builder_test::test_implicit_optional_for_option_type` | token-expansion unit | `RUST-REQ-082` |
| `crates/xmtp_macro/src/builder_test.rs` | `builder_test::test_explicit_optional_still_works` | token-expansion unit | `RUST-REQ-082` |
| `crates/xmtp_macro/src/timeout_macro_test.rs` | `timeout_macro_test::test_async_function_wraps_with_timeout` | token-expansion unit | `RUST-REQ-085` |
| `crates/xmtp_macro/src/timeout_macro_test.rs` | `timeout_macro_test::test_timeout_embeds_function_name_in_panic_message` | token-expansion unit | `RUST-REQ-085` |
| `crates/xmtp_macro/src/timeout_macro_test.rs` | `timeout_macro_test::test_non_async_function_returns_compile_error` | token-expansion unit | `RUST-REQ-085` |
| `crates/xmtp_proto/src/api_client/stats.rs` | `api_client::stats::tests::test_endpoint_stats_clear` | custom sync | `RUST-REQ-088` |
| `crates/xmtp_proto/src/api_client/stats.rs` | `api_client::stats::tests::test_endpoint_stats_display` | custom sync | `RUST-REQ-088` |
| `crates/xmtp_proto/src/impls/update_dedupe.rs` | `impls::update_dedupe::tests::test_dedupe` | custom async; unwrap_try | `RUST-REQ-089` |
| `crates/xmtp_proto/src/traits/combinators/ignore.rs` | `traits::combinators::ignore::tests::ignores_payloads` | custom async | `RUST-REQ-090` |
| `crates/xmtp_proto/src/traits/combinators/ignore.rs` | `traits::combinators::ignore::tests::ignore_is_retryable` | rstest fixture; cross-target async | `RUST-REQ-090` |
| `crates/xmtp_proto/src/traits/combinators/ignore.rs` | `traits::combinators::ignore::tests::ignore_is_orthogonal` | rstest fixture; cross-target async | `RUST-REQ-090` |
| `crates/xmtp_proto/src/traits/combinators/ignore.rs` | `traits::combinators::ignore::tests::endpoint_chains_work` | rstest fixture; cross-target async | `RUST-REQ-090` |
| `crates/xmtp_proto/src/traits/combinators/ignore.rs` | `traits::combinators::ignore::tests::endpoint_chains_orthogonal` | rstest fixture; cross-target async | `RUST-REQ-090` |
| `crates/xmtp_proto/src/traits/combinators/ignore.rs` | `traits::combinators::ignore::tests::test_body_delegates_to_wrapped_endpoint` | custom sync | `RUST-REQ-090` |
| `crates/xmtp_proto/src/traits/combinators/ignore.rs` | `traits::combinators::ignore::tests::test_grpc_endpoint_delegates_to_wrapped_endpoint` | custom sync | `RUST-REQ-090` |
| `crates/xmtp_proto/src/traits/combinators/retry.rs` | `traits::combinators::retry::tests::retries_endpoint_three_times` | custom async | `RUST-REQ-092` |
| `crates/xmtp_proto/src/traits/combinators/retry.rs` | `traits::combinators::retry::tests::does_not_retry_non_retryable` | custom async | `RUST-REQ-092` |
| `crates/xmtp_proto/src/traits/combinators/retry.rs` | `traits::combinators::retry::tests::test_grpc_endpoint_delegates_to_wrapped_endpoint` | custom sync | `RUST-REQ-092` |
| `crates/xmtp_proto/src/traits/combinators/retry.rs` | `traits::combinators::retry::tests::test_body_delegates_to_wrapped_endpoint` | custom sync | `RUST-REQ-092` |
| `crates/xmtp_proto/src/traits/combinators/retry.rs` | `traits::combinators::retry::tests::retries_with_strategy` | custom async | `RUST-REQ-092` |
| `crates/xmtp_proto/src/traits/combinators/v3_paged.rs` | `traits::combinators::v3_paged::tests::pages_endpoint` | rstest fixture; cross-target async | `RUST-REQ-093` |
| `crates/xmtp_proto/src/traits/combinators/v3_paged.rs` | `traits::combinators::v3_paged::tests::pages_endpoint_can_be_retried` | rstest fixture; cross-target async | `RUST-REQ-093` |
| `crates/xmtp_proto/src/traits/combinators/v3_paged.rs` | `traits::combinators::v3_paged::tests::test_grpc_endpoint_delegates_to_wrapped_endpoint` | custom sync | `RUST-REQ-093` |
| `crates/xmtp_proto/src/traits/combinators/v3_paged.rs` | `traits::combinators::v3_paged::tests::test_body_delegates_to_wrapped_endpoint` | custom sync | `RUST-REQ-093` |
| `crates/xmtp_proto/src/traits/combinators/v3_paged.rs` | `traits::combinators::v3_paged::tests::test_pageable_test_endpoint_body_encodes_protobuf_message` | custom sync | `RUST-REQ-093` |
| `crates/xmtp_proto/src/traits/combinators/v3_paged.rs` | `traits::combinators::v3_paged::tests::endpoints_can_be_chained` | custom async | `RUST-REQ-093`, `RUST-REQ-097` |
| `crates/xmtp_proto/src/traits/mock.rs` | `traits::mock::tests::test_grpc_endpoint_returns_empty_string` | custom sync | `RUST-REQ-094` |
| `crates/xmtp_proto/src/traits/short_hex.rs` | `traits::short_hex::tests::test_short_hex` | unit | `RUST-REQ-095` |
| `crates/xmtp_proto/src/traits/short_hex.rs` | `traits::short_hex::tests::test_short_hex_group_id` | unit | `RUST-REQ-095` |
| `crates/xmtp_proto/src/traits/stream.rs` | `traits::stream::tests::test_poll_next_successful_decode` | custom async | `RUST-REQ-096` |
| `crates/xmtp_proto/src/traits/stream.rs` | `traits::stream::tests::test_poll_next_error_mapping` | custom async | `RUST-REQ-096` |
| `crates/xmtp_proto/src/traits.rs` | `traits::test::endpoints_can_be_chained` | custom async | `RUST-REQ-097` |
| `crates/xmtp_proto/src/types/app_version.rs` | `types::app_version::tests::test_from_conversions` | rstest; 4 cases; cross-target async | `RUST-REQ-098` |
| `crates/xmtp_proto/src/types/app_version.rs` | `types::app_version::tests::test_metadata_value_conversion` | rstest; 3 cases; cross-target async | `RUST-REQ-098` |
| `crates/xmtp_proto/src/types/app_version.rs` | `types::app_version::tests::test_complex_versions` | rstest; 5 cases; cross-target async | `RUST-REQ-098` |
| `crates/xmtp_proto/src/types/cursor.rs` | `types::cursor::test::test_originator_constructors` | rstest; 6 cases; cross-target async | `RUST-REQ-099` |
| `crates/xmtp_proto/src/types/cursor.rs` | `types::cursor::test::test_ordering` | rstest; 4 cases; cross-target async | `RUST-REQ-099` |
| `crates/xmtp_proto/src/types/global_cursor.rs` | `types::global_cursor::tests::dominates_empty` | custom sync | `RUST-REQ-100` |
| `crates/xmtp_proto/src/types/group_message.rs` | `types::group_message::test::test_is_commit` | custom sync | `RUST-REQ-101` |
| `crates/xmtp_proto/src/types/group_message.rs` | `types::group_message::test::test_timestamp` | custom sync | `RUST-REQ-101` |
| `crates/xmtp_proto/src/types/ids/group_id.rs` | `types::ids::group_id::test::test_group_id_from_array` | rstest; 3 cases; cross-target async | `RUST-REQ-103` |
| `crates/xmtp_proto/src/types/ids/group_id.rs` | `types::ids::group_id::test::test_group_id_try_from_vec` | rstest; 4 cases; cross-target async | `RUST-REQ-103` |
| `crates/xmtp_proto/src/types/ids/group_id.rs` | `types::ids::group_id::test::test_group_id_try_from_slice` | rstest; 4 cases; cross-target async | `RUST-REQ-103` |
| `crates/xmtp_proto/src/types/ids/group_id.rs` | `types::ids::group_id::test::test_openmls_try_from_valid` | custom sync; unwrap_try | `RUST-REQ-103` |
| `crates/xmtp_proto/src/types/ids/group_id.rs` | `types::ids::group_id::test::test_openmls_try_from_wrong_length` | custom sync; unwrap_try | `RUST-REQ-103` |
| `crates/xmtp_proto/src/types/ids/group_id.rs` | `types::ids::group_id::test::test_to_openmls_roundtrip` | custom sync; unwrap_try | `RUST-REQ-103` |
| `crates/xmtp_proto/src/types/ids/group_id.rs` | `types::ids::group_id::test::test_fromstr_success` | custom sync; unwrap_try | `RUST-REQ-103` |
| `crates/xmtp_proto/src/types/ids/group_id.rs` | `types::ids::group_id::test::test_fromstr_bad_hex` | custom sync; unwrap_try | `RUST-REQ-103` |
| `crates/xmtp_proto/src/types/ids/group_id.rs` | `types::ids::group_id::test::test_fromstr_wrong_length` | custom sync; unwrap_try | `RUST-REQ-103` |
| `crates/xmtp_proto/src/types/ids/group_id.rs` | `types::ids::group_id::test::test_group_id_eq_vec` | rstest; 3 cases; cross-target async | `RUST-REQ-103` |
| `crates/xmtp_proto/src/types/ids/group_id.rs` | `types::ids::group_id::test::test_group_id_eq_array` | rstest; 2 cases; cross-target async | `RUST-REQ-103` |
| `crates/xmtp_proto/src/types/ids/group_id.rs` | `types::ids::group_id::test::test_group_id_eq_slice` | custom async; unwrap_try | `RUST-REQ-103` |
| `crates/xmtp_proto/src/types/ids/group_id.rs` | `types::ids::group_id::test::test_serde_roundtrip` | custom sync; unwrap_try | `RUST-REQ-103` |
| `crates/xmtp_proto/src/types/ids/group_id.rs` | `types::ids::group_id::test::test_serde_wrong_length_fails` | custom sync; unwrap_try | `RUST-REQ-103` |
| `crates/xmtp_proto/src/types/ids/group_id.rs` | `types::ids::group_id::test::test_generate_produces_16_bytes` | custom sync; unwrap_try | `RUST-REQ-103` |
| `crates/xmtp_proto/src/types/ids/group_id.rs` | `types::ids::group_id::test::test_default_is_zero` | custom sync; unwrap_try | `RUST-REQ-103` |
| `crates/xmtp_proto/src/types/ids/group_id.rs` | `types::ids::group_id::test::test_const_helpers` | custom sync; unwrap_try | `RUST-REQ-103` |
| `crates/xmtp_proto/src/types/ids/group_id.rs` | `types::ids::group_id::test::test_display_debug` | custom sync; unwrap_try | `RUST-REQ-103` |
| `crates/xmtp_proto/src/types/ids/group_id.rs` | `types::ids::group_id::test::diesel_test::test_diesel_roundtrip` | custom sync; unwrap_try; diesel feature; in-memory SQLite | `RUST-REQ-103` |
| `crates/xmtp_proto/src/types/ids/group_id.rs` | `types::ids::group_id::test::diesel_test::test_diesel_wrong_length_errors` | custom sync; unwrap_try; diesel feature; in-memory SQLite | `RUST-REQ-103` |
| `crates/xmtp_proto/src/types/ids/installation_id.rs` | `types::ids::installation_id::test::test_installation_id_from_array` | rstest; 3 cases; cross-target async | `RUST-REQ-107` |
| `crates/xmtp_proto/src/types/ids/installation_id.rs` | `types::ids::installation_id::test::test_installation_id_try_from_vec` | rstest; 4 cases; cross-target async | `RUST-REQ-107` |
| `crates/xmtp_proto/src/types/ids/installation_id.rs` | `types::ids::installation_id::test::test_installation_id_try_from_slice` | rstest; 4 cases; cross-target async | `RUST-REQ-107` |
| `crates/xmtp_proto/src/types/ids/installation_id.rs` | `types::ids::installation_id::test::test_installation_id_equality_with_vec` | rstest; 3 cases; cross-target async | `RUST-REQ-107` |
| `crates/xmtp_proto/src/types/ids/installation_id.rs` | `types::ids::installation_id::test::test_installation_id_equality_with_array` | rstest; 2 cases; cross-target async | `RUST-REQ-107` |
| `crates/xmtp_proto/src/types/ids/installation_id.rs` | `types::ids::installation_id::test::test_installation_id_equality_with_slice` | custom async | `RUST-REQ-107` |
| `crates/xmtp_proto/src/types/welcome_message.rs` | `types::welcome_message::test::test_accessor_methods` | rstest; 3 cases; cross-target async | `RUST-REQ-108` |
| `crates/xmtp_proto/src/types/welcome_message.rs` | `types::welcome_message::test::test_timestamp` | custom async | `RUST-REQ-108` |
| `crates/xmtp_proto/src/types.rs` | `types::tests::test_topic_kind_values` | rstest; 4 cases | `RUST-REQ-102` |
| `crates/wasm_macros/src/lib.rs` | `wasm_bindgen_numbered_enum (doctest line 23)` | rustdoc; ignored and not compiled; basic numbered-enum example only. | `RUST-REQ-087` |
| `crates/wasm_macros/src/lib.rs` | `wasm_bindgen_numbered_enum (doctest line 33)` | rustdoc; ignored and not compiled; additional-derive example only. | `RUST-REQ-087` |
| `crates/xmtp_common/src/error_code.rs` | `error_code (doctest line 8)` | rustdoc; ignored; enum plus inherited variant | `RUST-REQ-038` |
| `crates/xmtp_common/src/macros.rs` | `macros::wasm_or_native_expr (doctest line 41)` | rustdoc; ignored and not compiled; wasm-first invocation only | `RUST-REQ-054` |
| `crates/xmtp_common/src/retry.rs` | `retry (doctest line 5)` | rustdoc; ignored; retryable-derive sketch | `RUST-REQ-044` |
| `crates/xmtp_common/src/retry.rs` | `retry::RetryBuilder<S> (doctest line 238)` | rustdoc; ignored; five retries/custom strategy | `RUST-REQ-044`, `RUST-REQ-045` |
| `crates/xmtp_common/src/retry.rs` | `retry::retry_async (doctest line 282)` | executable async rustdoc; channel succeeds on value 2 | `RUST-REQ-044` |
| `crates/xmtp_logging/src/builder.rs` | `builder (doctest line 3)` | rustdoc; ignored; install and level update | `RUST-REQ-067`, `RUST-REQ-068` |
| `crates/xmtp_macro/src/builders.rs` | `builders::napi_builder (doctest line 19)` | rustdoc; ignored; all four field modes | `RUST-REQ-082` |
| `crates/xmtp_macro/src/error_code.rs` | `error_code::ErrorCodeAttr (doctest line 11)` | rustdoc; ignored; default and inherited codes | `RUST-REQ-038` |
| `crates/xmtp_macro/src/error_code.rs` | `error_code::ErrorCodeAttr (doctest line 39)` | rustdoc; ignored; custom compatibility code | `RUST-REQ-038` |
| `crates/xmtp_macro/src/lib.rs` | `derive_error_code (doctest line 137)` | rustdoc; ignored; default/inherited derive | `RUST-REQ-038` |
| `crates/xmtp_macro/src/lib.rs` | `derive_error_code (doctest line 165)` | rustdoc; ignored; renamed custom code | `RUST-REQ-038` |
| `crates/xmtp_macro/src/lib.rs` | `err_span (doctest line 294)` | rustdoc; ignored; NAPI async FFI example | `RUST-REQ-052` |
| `crates/xmtp_macro/src/lib.rs` | `napi_builder (doctest line 45)` | rustdoc; ignored; all field modes | `RUST-REQ-082` |
| `crates/xmtp_macro/src/lib.rs` | `rpc_span (doctest line 203)` | rustdoc; ignored; RPC operation naming | `RUST-REQ-052` |
| `crates/xmtp_macro/src/lib.rs` | `span (doctest line 257)` | rustdoc; ignored; custom stream prefix | `RUST-REQ-052` |
| `crates/xmtp_macro/src/lib.rs` | `test (doctest line 103)` | rustdoc; ignored; async test example | `RUST-REQ-053` |
| `crates/xmtp_macro/src/lib.rs` | `timeout (doctest line 186)` | rustdoc; ignored; 60-second async timeout | `RUST-REQ-085` |
| `crates/xmtp_macro/src/span_macro.rs` | `span_macro::expand_with_prefix (doctest line 10)` | rustdoc; ignored; canonical tracing attribute | `RUST-REQ-052` |
| `crates/xmtp_macro/src/test_macro.rs` | `test_macro::test (doctest line 18)` | rustdoc; ignored; async test example | `RUST-REQ-053` |
| `crates/xmtp_macro/src/timeout_macro.rs` | `timeout_macro::timeout (doctest line 13)` | rustdoc; ignored; async timeout example | `RUST-REQ-085` |
