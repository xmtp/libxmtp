# MLS client, identity, subscription, and worker test index

[← Test inventory](../existing-tests.md) · [Requirements](../existing-requirements.md)

This index contains 221 source-defined test declarations. Of these, 212 are reachable through the crate module tree. Nine rows are in orphan source files and do not currently run. A parameterized declaration is one row.

| File | Qualified test | Form / gates / cases | Requirements |
| --- | --- | --- | --- |
| crates/xmtp_mls/src/builder.rs | builder::worker_registration_tests::disabled_worker_is_not_registered | async XMTP test; ignored on wasm | `MLS-REQ-007` |
| crates/xmtp_mls/src/client.rs | client::tests::test_group_member_recovery | async XMTP | `MLS-REQ-008` |
| crates/xmtp_mls/src/client.rs | client::tests::test_client_error_signature_validation_retryability_propagates | sync XMTP | `MLS-REQ-009` |
| crates/xmtp_mls/src/client.rs | client::tests::test_mls_error | async XMTP | `MLS-REQ-010` |
| crates/xmtp_mls/src/client.rs | client::tests::test_register_installation | async XMTP | `MLS-REQ-011` |
| crates/xmtp_mls/src/client.rs | client::tests::test_rotate_key_package | async; wasm-bindgen on wasm; Tokio multi-thread on native | `MLS-REQ-012` |
| crates/xmtp_mls/src/client.rs | client::tests::test_find_groups | async XMTP | `MLS-REQ-013` |
| crates/xmtp_mls/src/client.rs | client::tests::test_find_inbox_id | async XMTP | `MLS-REQ-014` |
| crates/xmtp_mls/src/client.rs | client::tests::test_double_dms | async XMTP | `MLS-REQ-015` |
| crates/xmtp_mls/src/client.rs | client::tests::only_test_sync_welcomes | async rstest and XMTP | `MLS-REQ-016` |
| crates/xmtp_mls/src/client.rs | client::tests::test_leaf_node_lifetime_validation_disabled | async XMTP multi-thread; native only | `MLS-REQ-017` |
| crates/xmtp_mls/src/client.rs | client::tests::test_sync_all_groups | async rstest and XMTP; 10 worker threads | `SHARED-GROUP-REQ-038` |
| crates/xmtp_mls/src/client.rs | client::tests::test_sync_all_groups_and_welcomes | async XMTP multi-thread | `MLS-REQ-019` |
| `crates/xmtp_mls/src/client.rs` | `client::tests::test_sync_100_allowed_groups_performance` | async XMTP plus wasm; creates 100 invites, discards sync count, samples only the first group for one welcome, and asserts no time limit | `SHARED-GROUP-REQ-038` |
| crates/xmtp_mls/src/client.rs | client::tests::test_add_remove_then_add_again | async rstest and XMTP | `SHARED-GROUP-REQ-015` |
| crates/xmtp_mls/src/client.rs | client::tests::test_key_package_rotation | async XMTP; worker timing | `MLS-REQ-012` |
| crates/xmtp_mls/src/client.rs | client::tests::test_find_or_create_dm_by_inbox_id | async XMTP | `MLS-REQ-015` |
| crates/xmtp_mls/src/client.rs | client::tests::should_stream_consent | async XMTP | `SHARED-SYNC-REQ-008` |
| crates/xmtp_mls/src/client.rs | client::tests::should_reconnect | async rstest and XMTP; ignored on wasm; toxiproxy; 100-second cap | `MLS-REQ-023` |
| crates/xmtp_mls/src/client.rs | client::tests::test_list_conversations_pagination | async rstest and XMTP | `SHARED-GROUP-REQ-037` |
| crates/xmtp_mls/src/client.rs | client::tests::test_delete_message | async XMTP | `MLS-REQ-025` |
| crates/xmtp_mls/src/client.rs | client::tests::close_stops_workers | async XMTP | `SHARED-IDENTITY-REQ-019` |
| crates/xmtp_mls/src/client.rs | client::tests::close_is_idempotent | async XMTP | `SHARED-IDENTITY-REQ-019` |
| crates/xmtp_mls/src/client.rs | client::tests::close_disconnects_db | async XMTP; ignored on wasm; persistent database | `SHARED-IDENTITY-REQ-019` |
| crates/xmtp_mls/src/client.rs | client::tests::close_cancels_callback_stream | async XMTP; ignored on wasm | `SHARED-IDENTITY-REQ-019` |
| crates/xmtp_mls/src/client.rs | client::tests::reconnect_after_close_errors | async XMTP | `SHARED-IDENTITY-REQ-019` |
| crates/xmtp_mls/src/identity.rs | identity::tests::ensure_pq_keys_are_deleted | async XMTP | `MLS-REQ-027` |
| crates/xmtp_id/src/key_package/construction.rs | key_package::construction::tests::generated_package_preserves_options_and_verifies | sync XMTP | `MLS-REQ-028` |
| crates/xmtp_mls/src/identity.rs | identity::tests::test_generate_post_quantum_key_error_codes | plain sync | `MLS-REQ-029` |
| crates/xmtp_mls/src/identity.rs | identity::tests::test_identity_error_codes | plain sync; many enum variants | `MLS-REQ-029` |
| crates/xmtp_mls/src/identity.rs | identity::tests::test_identity_error_inherited_codes | plain sync | `MLS-REQ-029` |
| crates/xmtp_mls/src/identity.rs | identity::tests::post_quantum_interop | async XMTP; four PQ and legacy combinations | `MLS-REQ-030` |
| crates/xmtp_mls/src/identity_updates.rs | identity_updates::tests::test_is_member_of_association_state | async rstest and XMTP | `MLS-REQ-031` |
| crates/xmtp_mls/src/identity_updates.rs | identity_updates::tests::create_inbox_round_trip | async rstest and XMTP | `MLS-REQ-032` |
| crates/xmtp_mls/src/identity_updates.rs | identity_updates::tests::add_association | async rstest and XMTP | `MLS-REQ-033` |
| crates/xmtp_mls/src/identity_updates.rs | identity_updates::tests::cache_association_state | native-only cfg-generated plain test; traced async body | `MLS-REQ-034` |
| crates/xmtp_mls/src/identity_updates.rs | identity_updates::tests::load_identity_updates_if_needed | async rstest and XMTP | `MLS-REQ-035` |
| crates/xmtp_mls/src/identity_updates.rs | identity_updates::tests::get_installation_diff | async rstest and XMTP | `MLS-REQ-036` |
| crates/xmtp_mls/src/identity_updates.rs | identity_updates::tests::get_installation_diff_rejects_added_inbox_at_sequence_zero | async rstest and XMTP | `MLS-REQ-037` |
| crates/xmtp_mls/src/identity_updates.rs | identity_updates::tests::revoke_wallet | async rstest and XMTP | `MLS-REQ-038` |
| crates/xmtp_mls/src/identity_updates.rs | identity_updates::tests::revoke_installation | async rstest and XMTP | `MLS-REQ-039` |
| crates/xmtp_mls/src/identity_updates.rs | identity_updates::tests::revoke_installation_with_malformed_keypackage | Tokio multi-thread; native only | `MLS-REQ-039` |
| crates/xmtp_mls/src/identity_updates.rs | identity_updates::tests::revoke_good_installation_with_other_malformed_keypackage | Tokio multi-thread; native only | `MLS-REQ-039` |
| crates/xmtp_mls/src/identity_updates.rs | identity_updates::tests::change_recovery_address | async rstest and XMTP | `MLS-REQ-040` |
| crates/xmtp_mls/src/test/builder.rs | test::builder::builder_test | async XMTP | `MLS-REQ-001` |
| crates/xmtp_mls/src/test/builder.rs | test::builder::test_client_creation | async XMTP; six table cases in body | `MLS-REQ-001` |
| crates/xmtp_mls/src/test/builder.rs | test::builder::test_2nd_time_client_creation | async XMTP | `MLS-REQ-003` |
| crates/xmtp_mls/src/test/builder.rs | test::builder::api_identity_mismatch | async XMTP; mocked API | `MLS-REQ-003` |
| crates/xmtp_mls/src/test/builder.rs | test::builder::api_identity_happy_path | async XMTP; mocked API | `MLS-REQ-003` |
| crates/xmtp_mls/src/test/builder.rs | test::builder::stored_identity_happy_path | async XMTP | `MLS-REQ-003` |
| crates/xmtp_mls/src/test/builder.rs | test::builder::stored_identity_mismatch | async XMTP | `MLS-REQ-003` |
| crates/xmtp_mls/src/test/builder.rs | test::builder::identity_persistence_test | async XMTP; persistent store reopen | `MLS-REQ-003` |
| crates/xmtp_mls/src/test/builder_native_only.rs | test::builder_native_only::test_remote_is_valid_signature | async rstest and Tokio; native module; Docker SCW; 60-second cap | `MLS-REQ-005` |
| crates/xmtp_mls/src/test/builder_native_only.rs | test::builder_native_only::test_detect_scw_vs_eoa_creation | async rstest and Tokio; native module; Docker SCW; 60-second cap | `MLS-REQ-005` |
| crates/xmtp_mls/src/test/builder_native_only.rs | test::builder_native_only::test_two_smart_contract_wallets_group_messaging | async rstest and Tokio; native module; Docker SCW | `MLS-REQ-005` |
| crates/xmtp_mls/src/test/builder_native_only.rs | test::builder_native_only::test_invalid_scw_prevents_db_storage | async rstest and Tokio; native module; verifier false | `MLS-REQ-005` |
| crates/xmtp_mls/src/test/builder_native_only.rs | test::builder_native_only::test_invalid_scw_then_valid_scw_recovery | async rstest and Tokio; native module; false then remote verifier | `MLS-REQ-005` |
| crates/xmtp_mls/src/test/builder_native_only.rs | test::builder_native_only::test_operations_fail_when_not_ready | async XMTP; native module | `MLS-REQ-006` |
| crates/xmtp_mls/src/utils/cleanup_duplicate_updates.rs | utils::cleanup_duplicate_updates::tests::test_cleanup_works_as_expected | async XMTP | `MLS-REQ-043` |
| crates/xmtp_mls/src/utils/test/tester_utils.rs | utils::test::tester_utils::tests::test_snapshots | async XMTP | `MLS-REQ-044` |
| crates/xmtp_mls/src/worker.rs | worker::disconnect_propagation_tests::group_error_forwards_disconnect | sync XMTP; native-only module | `MLS-REQ-048` |
| crates/xmtp_mls/src/worker.rs | worker::disconnect_propagation_tests::mls_store_error_forwards_disconnect | sync XMTP; native-only module | `MLS-REQ-048` |
| crates/xmtp_mls/src/worker.rs | worker::disconnect_propagation_tests::subscribe_error_forwards_disconnect | sync XMTP; native-only module | `MLS-REQ-048` |
| crates/xmtp_mls/src/worker.rs | worker::disconnect_propagation_tests::task_worker_load_group_forwards_disconnect | sync XMTP; native-only module | `MLS-REQ-048` |
| crates/xmtp_mls/src/worker.rs | worker::disconnect_propagation_tests::commit_log_error_forwards_disconnect | sync XMTP; native-only module | `MLS-REQ-048` |
| crates/xmtp_mls/src/worker.rs | worker::disconnect_propagation_tests::device_sync_error_forwards_disconnect | sync XMTP; native-only module | `MLS-REQ-048` |
| crates/xmtp_mls/src/worker.rs | worker::disconnect_propagation_tests::key_package_maintenance_error_forwards_disconnect | sync XMTP; native-only module | `MLS-REQ-048` |
| crates/xmtp_mls/src/worker.rs | worker::worker_config_tests::default_is_all_enabled_no_overrides | sync XMTP | `MLS-REQ-049` |
| crates/xmtp_mls/src/worker.rs | worker::worker_config_tests::per_kind_override_beats_global_default | sync XMTP | `MLS-REQ-049` |
| crates/xmtp_mls/src/worker.rs | worker::worker_config_tests::zero_resolved_base_clamps_to_const | sync XMTP | `MLS-REQ-049` |
| crates/xmtp_mls/src/worker.rs | worker::worker_config_tests::per_kind_jitter_is_carried | sync XMTP | `MLS-REQ-049` |
| crates/xmtp_mls/src/worker.rs | worker::worker_config_tests::jitter_is_scoped_per_worker | sync XMTP | `MLS-REQ-049` |
| crates/xmtp_mls/src/worker.rs | worker::worker_config_tests::disabled_entry_reports_false | sync XMTP | `MLS-REQ-049` |
| crates/xmtp_mls/src/worker/disappearing_messages.rs | worker::disappearing_messages::tests::rearm_delivers_a_signal | async XMTP | `MLS-REQ-050` |
| crates/xmtp_mls/src/worker/tasks.rs | worker::tasks::tests::done_deletes | async XMTP; TaskRunner disabled | `MLS-REQ-051` |
| crates/xmtp_mls/src/worker/tasks.rs | worker::tasks::tests::add_missing_installations_missing_group_deletes_task | async XMTP; TaskRunner disabled | `MLS-REQ-051` |
| crates/xmtp_mls/src/worker/tasks.rs | worker::tasks::tests::recurring_task_advances_and_does_not_hot_loop | async XMTP; reschedule hook | `MLS-REQ-052` |
| crates/xmtp_mls/src/worker/tasks.rs | worker::tasks::tests::never_expire_seed_survives_reaper | async XMTP; high attempts | `MLS-REQ-052` |
| crates/xmtp_mls/src/worker/tasks.rs | worker::tasks::tests::not_yet_due_task_is_not_run_early | async XMTP; plus 30-day row | `MLS-REQ-052` |
| crates/xmtp_mls/src/worker/tasks.rs | worker::tasks::tests::pull_in_arm_lowers_existing_target | async XMTP; direct dispatch | `MLS-REQ-053` |
| crates/xmtp_mls/src/worker/tasks.rs | worker::tasks::tests::pull_in_task_runs_and_pulls_in | async XMTP; live TaskRunner | `MLS-REQ-053` |
| crates/xmtp_mls/src/worker/key_package_maintenance.rs | worker::key_package_maintenance::tests::kp_errors_forward_db_reconnect | sync XMTP; native-only module | `MLS-REQ-048` |
| crates/xmtp_mls/src/worker/key_package_maintenance.rs | worker::key_package_maintenance::tests::manual_rotation_nudges_deletion | async XMTP; TaskRunner disabled | `MLS-REQ-056` |
| crates/xmtp_mls/src/worker/key_package_maintenance.rs | worker::key_package_maintenance::tests::rotation_task_rotates_and_reschedules | async XMTP; due after 6 seconds | `MLS-REQ-055` |
| crates/xmtp_mls/src/worker/key_package_maintenance.rs | worker::key_package_maintenance::tests::rotation_ensures_and_pulls_in_deletion_when_singleton_missing | async XMTP; missing deletion seed | `MLS-REQ-055`, `MLS-REQ-056` |
| crates/xmtp_mls/src/worker/key_package_maintenance.rs | worker::key_package_maintenance::tests::deletion_task_sweeps_and_reschedules | async XMTP; waits past grace | `MLS-REQ-056` |
| crates/xmtp_mls/src/worker/key_package_maintenance.rs | worker::key_package_maintenance::tests::kp_tasks_seeded_when_workers_run_absent_when_passive | async XMTP; runner on and off | `MLS-REQ-054` |
| crates/xmtp_mls/src/worker/key_package_maintenance.rs | worker::key_package_maintenance::tests::startup_reconcile_pulls_in_far_scheduled_row | async XMTP; stale plus 30-day row | `MLS-REQ-054` |
| crates/xmtp_mls/src/worker/key_package_maintenance.rs | worker::key_package_maintenance::tests::rotation_task_not_due_reschedules_without_rotating | async XMTP; normal far deadline | `MLS-REQ-055` |
| crates/xmtp_mls/src/worker/key_package_maintenance.rs | worker::key_package_maintenance::tests::welcome_nudge_selfheals_missing_rotation_seed | async XMTP; runner disabled | `MLS-REQ-054` |
| crates/xmtp_mls/src/worker/key_package_maintenance.rs | worker::key_package_maintenance::tests::welcome_nudge_pulls_in_parked_rotation | async XMTP; parked row | `MLS-REQ-054` |
| crates/xmtp_mls/src/worker/device_sync/archive.rs | worker::device_sync::archive::tests::test_archive_timestamps | async XMTP; existing and missing target group | `MLS-REQ-057` |
| crates/xmtp_mls/src/worker/device_sync/archive.rs | worker::device_sync::archive::tests::test_dm_archive | async XMTP | `SHARED-SYNC-REQ-003` |
| crates/xmtp_mls/src/worker/device_sync/archive.rs | worker::device_sync::archive::tests::test_buffer_export_import | async rstest and XMTP | `SHARED-SYNC-REQ-002` |
| crates/xmtp_mls/src/worker/device_sync/archive.rs | worker::device_sync::archive::tests::test_file_backup | async XMTP; native only; file I/O | `SHARED-SYNC-REQ-002` |
| crates/xmtp_mls/src/worker/device_sync/archive.rs | worker::device_sync::archive::tests::test_legacy_archive_import | async XMTP; native only; fixture | `MLS-REQ-060` |
| crates/xmtp_mls/src/worker/device_sync/archive.rs | worker::device_sync::archive::tests::test_archive_includes_migrated_groups | async XMTP; migrated and legacy groups | `MLS-REQ-061` |
| crates/xmtp_mls/src/worker/device_sync/preference_sync.rs | worker::device_sync::preference_sync::tests::test_hmac_sync | async rstest and XMTP | `MLS-REQ-062` |
| crates/xmtp_mls/src/worker/device_sync/tests.rs | worker::device_sync::tests::test_hmac_and_consent_preference_sync | async rstest and XMTP; ignored on wasm; compares only the first of three HMAC keys, then verifies Denied DM and Allowed group consent propagation | `MLS-REQ-070` |
| crates/xmtp_mls/src/worker/device_sync/tests.rs | worker::device_sync::tests::test_only_added_to_correct_groups | async rstest and XMTP; ignored on wasm | `MLS-REQ-071` |
| crates/xmtp_mls/src/worker/device_sync/tests.rs | worker::device_sync::tests::test_new_devices_not_added_to_old_sync_groups | async rstest and XMTP; ignored on wasm; 15-second cap | `MLS-REQ-072` |
| crates/xmtp_mls/src/worker/device_sync/tests.rs | worker::device_sync::tests::test_incremental_consent | async rstest and XMTP; ignored on wasm; 60-second cap | `MLS-REQ-074` |
| crates/xmtp_mls/src/worker/device_sync/tests.rs | worker::device_sync::tests::test_task_runner_adds_new_installation_to_groups | async rstest and XMTP; ignored on wasm; live worker | `MLS-REQ-075` |
| crates/xmtp_mls/src/worker/device_sync/tests.rs | worker::device_sync::tests::test_sync_group_creation_leaves_no_reconcile_task | async rstest and XMTP; ignored on wasm; TaskRunner disabled | `MLS-REQ-075` |
| crates/xmtp_mls/src/worker/device_sync/tests.rs | worker::device_sync::tests::test_welcome_schedules_add_installation_tasks | async rstest and XMTP; ignored on wasm; first schedule creates at least one matching task; second identical schedule leaves total add-task count unchanged | `MLS-REQ-075` |
| crates/xmtp_mls/src/subscriptions/process_message.rs | subscriptions::process_message::tests::test_process_returns_correct_cursor | async rstest and XMTP; values 5, 8, 10, 11, 13, and 18 | `MLS-REQ-078` |
| crates/xmtp_mls/src/subscriptions/process_message.rs | subscriptions::process_message::tests::test_process_returns_correct_cursor_on_err | async XMTP with seven-case rstest-reuse template | `MLS-REQ-078` |
| crates/xmtp_mls/src/subscriptions/process_message.rs | subscriptions::process_message::tests::test_process_surfaces_decrypt_between_failed_cursors | async XMTP; regression cursors 10, 11, and 12 | `MLS-REQ-079` |
| crates/xmtp_mls/src/subscriptions/process_message.rs | subscriptions::process_message::tests::test_cursor_no_sync | async rstest and XMTP; stored message None or Some | `MLS-REQ-078` |
| crates/xmtp_mls/src/subscriptions/process_message.rs | subscriptions::process_message::tests::process_one_uses_db_fast_path_without_decrypting | async XMTP; stub factory | `MLS-REQ-080` |
| crates/xmtp_mls/src/subscriptions/process_message.rs | subscriptions::process_message::tests::process_one_runs_pipeline_on_fast_path_miss | async XMTP; stub factory | `MLS-REQ-080` |
| crates/xmtp_mls/src/subscriptions/process_welcome.rs | subscriptions::process_welcome::tests::into_outcome_ignore_yields_nothing | async XMTP | `MLS-REQ-081` |
| crates/xmtp_mls/src/subscriptions/process_welcome.rs | subscriptions::process_welcome::tests::into_outcome_ignore_id_records_seen_without_group | async XMTP | `MLS-REQ-081` |
| crates/xmtp_mls/src/subscriptions/stream_messages.rs | subscriptions::stream_messages::tests::test_stream_messages | async rstest and XMTP; ignored on wasm; 30-second cap; test applies its own Application-kind filter before asserting `hello` then `hello2` | `SHARED-SYNC-REQ-005` |
| crates/xmtp_mls/src/subscriptions/stream_messages/stream_stats.rs | subscriptions::stream_messages::stream_stats::tests::test_stream_stats | async XMTP | `MLS-REQ-083` |
| crates/xmtp_mls/src/subscriptions/stream_conversations.rs | subscriptions::stream_conversations::test::stream_welcomes | async rstest and XMTP; cases 2 and 5; async fixtures | `SHARED-GROUP-REQ-028` |
| crates/xmtp_mls/src/subscriptions/stream_conversations.rs | subscriptions::stream_conversations::test::test_sync_groups_are_not_streamed | async rstest and XMTP | `SHARED-GROUP-REQ-028` |
| crates/xmtp_mls/src/subscriptions/stream_conversations.rs | subscriptions::stream_conversations::test::test_dm_stream_filter | async rstest and XMTP; DM and Group cases; ignored on wasm | `SHARED-GROUP-REQ-028` |
| crates/xmtp_mls/src/subscriptions/stream_conversations.rs | subscriptions::stream_conversations::test::test_dm_stream_all_conversation_types | async rstest and XMTP | `SHARED-GROUP-REQ-028` |
| crates/xmtp_mls/src/subscriptions/stream_conversations.rs | subscriptions::stream_conversations::test::test_self_group_creation | async rstest and XMTP; 10-second cap | `SHARED-GROUP-REQ-028` |
| crates/xmtp_mls/src/subscriptions/stream_conversations.rs | subscriptions::stream_conversations::test::test_add_remove_re_add | async rstest and XMTP; 5-second cap | `MLS-REQ-086` |
| crates/xmtp_mls/src/subscriptions/stream_conversations.rs | subscriptions::stream_conversations::test::test_duplicate_dm_not_streamed | async rstest and XMTP; 15-second cap | `MLS-REQ-087` |
| `crates/xmtp_mls/src/subscriptions/stream_conversations.rs` | `subscriptions::stream_conversations::test::test_many_concurrent_dm_invites` | async rstest and XMTP; cases 5 and 100 plus wasm; 120 seconds; discards task handles and N stream-poll Option/Result/value outputs | `MLS-REQ-088` |
| crates/xmtp_mls/src/subscriptions/stream_all/tests.rs | subscriptions::stream_all::tests::test_stream_all_messages_changing_group_list | async rstest and XMTP; ignored on wasm | `MLS-REQ-089` |
| crates/xmtp_mls/src/subscriptions/stream_all/tests.rs | subscriptions::stream_all::tests::test_stream_all_messages_unchanging_group_list | async rstest and XMTP | `MLS-REQ-089` |
| crates/xmtp_mls/src/subscriptions/stream_all/tests.rs | subscriptions::stream_all::tests::test_dm_stream_all_messages | async rstest and XMTP | `SHARED-GROUP-REQ-030` |
| `crates/xmtp_mls/src/subscriptions/stream_all/tests.rs` | `subscriptions::stream_all::tests::test_stream_all_messages_does_not_lose_messages` | async rstest and XMTP; ignored on wasm; 45 messages | `MLS-REQ-089` |
| crates/xmtp_mls/src/subscriptions/stream_all/tests.rs | subscriptions::stream_all::tests::test_stream_all_messages_detached_group_changes | async rstest and XMTP; five new groups | `MLS-REQ-089` |
| crates/xmtp_mls/src/subscriptions/stream_all/tests.rs | subscriptions::stream_all::tests::test_stream_all_messages_filters_by_consent_state | async rstest and XMTP; Allowed, Denied, and Unknown; ignored on wasm | `SHARED-GROUP-REQ-030` |
| crates/xmtp_mls/src/subscriptions/stream_all/tests.rs | subscriptions::stream_all::tests::stream_messages_keeps_track_of_cursor | async rstest and XMTP; old epochs and new installation | `MLS-REQ-092` |
| crates/xmtp_mls/src/subscriptions/stream_all/tests.rs | subscriptions::stream_all::tests::test_stream_all_messages_filters_conversations_created_after_init | async rstest and XMTP; Allowed filter | `SHARED-GROUP-REQ-030` |
| crates/xmtp_mls/src/subscriptions/stream_all/tests.rs | subscriptions::stream_all::tests::test_stream_all_messages_filters_new_group_when_dm_only | async rstest and XMTP; DM-only | `SHARED-GROUP-REQ-030` |
| crates/xmtp_mls/src/subscriptions/stream_all/tests.rs | subscriptions::stream_all::tests::test_stream_all_messages_respects_cursor_between_streams | async rstest and XMTP; messages 1, 2, and 3 | `MLS-REQ-092` |
| crates/xmtp_mls/src/subscriptions/stream_all/tests.rs | subscriptions::stream_all::tests::test_stream_all_concurrent_writes | async rstest and XMTP multi-thread; ignored on wasm; 100 messages | `MLS-REQ-089` |
| crates/xmtp_mls/src/subscriptions/stream_all/tests.rs | subscriptions::stream_all::tests::test_new_group_does_not_duplicate_messages | async XMTP; ignored on wasm; 50 old groups and one new group; requires only fewer than 5 new processed-stat entries, with IDs/content unchecked | `MLS-REQ-089` |
| crates/xmtp_mls/src/subscriptions/stream_all/tests.rs | subscriptions::stream_all::tests::watchdog_trips_on_idle_real_stream | async XMTP; ignored on wasm; real v3 stream | `MLS-REQ-095` |
| crates/xmtp_mls/src/subscriptions/stream_all/tests.rs | subscriptions::stream_all::tests::watchdog_reconnect_keeps_stream_alive | native-only cfg-generated plain test; traced async body; stale trip plus later `second` delivery; no cursor or replay assertion | `MLS-REQ-095` |
| crates/xmtp_mls/src/subscriptions/watchdog.rs | subscriptions::watchdog::tests::passes_items_then_trips_once_when_idle | plain sync; manual timer | `MLS-REQ-093` |
| crates/xmtp_mls/src/subscriptions/watchdog.rs | subscriptions::watchdog::tests::clean_end_is_not_a_trip | plain sync; manual timer | `MLS-REQ-093` |
| crates/xmtp_mls/src/subscriptions/watchdog.rs | subscriptions::watchdog::tests::does_not_trip_until_timer_fires | plain sync; repeated polls | `MLS-REQ-093` |
| crates/xmtp_mls/src/subscriptions/watchdog.rs | subscriptions::watchdog::tests::yields_every_item_then_exactly_one_stale | proptest property; arbitrary byte vectors with length 0..32 exclusive | `MLS-REQ-093` |
| crates/xmtp_mls/src/subscriptions/watchdog.rs | subscriptions::watchdog::tests::new_uses_a_real_timer | async XMTP; 50-ms timer | `MLS-REQ-093` |
| crates/xmtp_mls/src/subscriptions/watchdog.rs | subscriptions::watchdog::tests::disabled_watchdog_never_trips | async XMTP; 100-ms observation | `MLS-REQ-093` |
| crates/xmtp_mls/src/subscriptions/watchdog.rs | subscriptions::watchdog::tests::config_reads_env_with_defaults | plain sync; injected lookup | `MLS-REQ-094` |
| crates/xmtp_mls/src/subscriptions/watchdog.rs | subscriptions::watchdog::tests::watchdog_is_opt_in | plain sync; boolean spellings | `MLS-REQ-094` |
| crates/xmtp_mls/src/subscriptions/watchdog.rs | subscriptions::watchdog::tests::config_clamps_oversized_env_values | plain sync; u64::MAX | `MLS-REQ-094` |
| crates/xmtp_mls/src/subscriptions/watchdog.rs | subscriptions::watchdog::tests::rand_jitter_zero_is_zero | plain sync | `MLS-REQ-094` |
| crates/xmtp_mls/src/subscriptions/watchdog.rs | subscriptions::watchdog::tests::rand_jitter_stays_in_bounds | plain sync; 1,000 draws | `MLS-REQ-094` |
| crates/xmtp_mls/src/subscriptions/watchdog.rs | subscriptions::watchdog::tests::throttle_is_immediate_after_long_idle | plain sync; elapsed 300 seconds and exact floor | `MLS-REQ-094` |
| crates/xmtp_mls/src/subscriptions/watchdog.rs | subscriptions::watchdog::tests::throttle_caps_a_tight_loop | plain sync; 50 ms of 2-second floor | `MLS-REQ-094` |
| crates/xmtp_mls/src/subscriptions/catch_up.rs | subscriptions::catch_up::tests::catch_up_joins_pending_groups_and_stores_history | async XMTP; native v3 module | `SHARED-GROUP-REQ-039` |
| crates/xmtp_mls/src/subscriptions/catch_up.rs | subscriptions::catch_up::tests::catch_up_replays_the_missed_tail_idempotently | async XMTP; native v3 module; repeated run | `SHARED-GROUP-REQ-039` |
| crates/xmtp_mls/src/subscriptions/catch_up.rs | subscriptions::catch_up::tests::catch_up_with_nothing_owed_completes | async XMTP; native v3 module | `SHARED-GROUP-REQ-039` |
| crates/xmtp_mls/src/subscriptions/catch_up.rs | subscriptions::catch_up::tests::legacy_catch_up_counts_the_same_way | async XMTP; native v3 module; repeated run | `SHARED-GROUP-REQ-039` |
| `crates/xmtp_mls/src/subscriptions/catch_up.rs` | `subscriptions::catch_up::plan_tests::plan_splits_a_large_subscription_set_into_bounded_updates` | sync XMTP; 5,000 topics | `MLS-REQ-101` |
| `crates/xmtp_mls/src/subscriptions/catch_up.rs` | `subscriptions::catch_up::plan_tests::plan_keeps_a_small_set_in_one_update` | sync XMTP; three topics | `MLS-REQ-101` |
| crates/xmtp_mls/src/subscriptions/bidi_tests.rs | subscriptions::bidi_tests::bidi_connection_delivers_live_welcome_over_the_wire | async XMTP; native v3 module; live backend | `MLS-REQ-096` |
| `crates/xmtp_mls/src/subscriptions/bidi_tests.rs` | `subscriptions::bidi_tests::bidi_reaches_applied_target_then_streams_live` | async XMTP; native v3; 5 plus 3 plus 4 messages | `MLS-REQ-097` |
| `crates/xmtp_mls/src/subscriptions/bidi_fuzz_tests.rs` | `subscriptions::bidi_fuzz_tests::fuzz_server_honors_update_acknowledgements_and_targets` | async fuzz-style XMTP; native v3; seed and rounds environment; 300 seconds | `MLS-REQ-102` |
| crates/xmtp_mls/src/subscriptions/bidi_fuzz_tests.rs | subscriptions::bidi_fuzz_tests::fuzz_transport_delivery_never_loses_above_the_floor | async fuzz-style XMTP; native v3; toxiproxy; seed and rounds environment; 300 seconds | `MLS-REQ-103` |
| crates/xmtp_mls/src/subscriptions/stream_router.rs | subscriptions::stream_router::tests::window_dedups_by_stored_identity_only | async XMTP; stream-router module is native | `MLS-REQ-106` |
| crates/xmtp_mls/src/subscriptions/stream_router.rs | subscriptions::stream_router::tests::windows_close_per_topic | async XMTP; native | `MLS-REQ-106` |
| crates/xmtp_mls/src/subscriptions/stream_router.rs | subscriptions::stream_router::tests::surfaced_ahead_outlives_the_window | async XMTP; native | `MLS-REQ-106` |
| crates/xmtp_mls/src/subscriptions/stream_router.rs | subscriptions::stream_router::tests::growth_lease_folds_stored_identities_into_open_windows | async XMTP; native | `MLS-REQ-106` |
| crates/xmtp_mls/src/subscriptions/stream_router_tests.rs | subscriptions::stream_router_tests::router_delivers_live_messages | async XMTP; native v3 module | `MLS-REQ-104` |
| crates/xmtp_mls/src/subscriptions/stream_router_tests.rs | subscriptions::stream_router_tests::router_catches_up_from_durable_cursor | async XMTP; native v3 module | `MLS-REQ-104` |
| crates/xmtp_mls/src/subscriptions/stream_router_tests.rs | subscriptions::stream_router_tests::resubscribe_does_not_redeliver | async XMTP; native v3 module | `MLS-REQ-104` |
| crates/xmtp_mls/src/subscriptions/stream_router_tests.rs | subscriptions::stream_router_tests::sibling_conversation_streams_both_receive_a_welcome | async XMTP; native v3 module | `MLS-REQ-105` |
| crates/xmtp_mls/src/subscriptions/stream_router_tests.rs | subscriptions::stream_router_tests::a_panicked_welcome_task_surfaces_instead_of_parking | async XMTP; native v3 module; injected panic | `MLS-REQ-115` |
| crates/xmtp_mls/src/subscriptions/router_callbacks_tests.rs | subscriptions::router_callbacks_tests::welcomed_group_joins_the_live_stream | async XMTP; native v3 module | `MLS-REQ-107` |
| crates/xmtp_mls/src/subscriptions/router_callbacks_tests.rs | subscriptions::router_callbacks_tests::self_created_group_streams_its_messages | async XMTP; native v3 module | `MLS-REQ-107` |
| crates/xmtp_mls/src/subscriptions/router_callbacks_tests.rs | subscriptions::router_callbacks_tests::self_created_conversation_surfaces_on_the_stream | async XMTP; native v3 module | `MLS-REQ-107` |
| crates/xmtp_mls/src/subscriptions/router_callbacks_tests.rs | subscriptions::router_callbacks_tests::callback_stream_delivers_live_messages | async XMTP; native v3 module | `MLS-REQ-107` |
| crates/xmtp_mls/src/subscriptions/router_callbacks_tests.rs | subscriptions::router_callbacks_tests::callback_stream_surfaces_new_conversations | async XMTP; native v3 module | `MLS-REQ-107` |
| crates/xmtp_mls/src/subscriptions/router_callbacks_tests.rs | subscriptions::router_callbacks_tests::sibling_clients_share_the_process_transport | async XMTP; native v3 module | `MLS-REQ-108` |
| crates/xmtp_mls/src/subscriptions/router_callbacks_tests.rs | subscriptions::router_callbacks_tests::single_conversation_callback_is_scoped_to_its_group | async XMTP; native v3 module | `MLS-REQ-108` |
| crates/xmtp_mls/src/subscriptions/router_callbacks_tests.rs | subscriptions::router_callbacks_tests::suspend_resume_replays_what_was_missed | async XMTP; native v3 module; two cycles | `MLS-REQ-109` |
| crates/xmtp_mls/src/subscriptions/router_callbacks_tests.rs | subscriptions::router_callbacks_tests::suspend_before_the_first_stream_parks_the_wire | async XMTP; native v3 module; process-isolated | `MLS-REQ-109` |
| crates/xmtp_mls/src/subscriptions/router_callbacks_tests.rs | subscriptions::router_callbacks_tests::lifecycle_helpers_are_noops_without_a_transport | async XMTP; native v3 module; process-isolated | `MLS-REQ-109` |
| crates/xmtp_mls/src/subscriptions/router_callbacks_tests.rs | subscriptions::router_callbacks_tests::sync_group_messages_are_intercepted_not_delivered | async XMTP; native v3 module; device-sync worker | `MLS-REQ-110` |
| crates/xmtp_mls/src/subscriptions/router_callbacks_tests.rs | subscriptions::router_callbacks_tests::stream_all_with_no_conversations_stays_open | async XMTP; native v3 module; empty account | `MLS-REQ-107` |
| crates/xmtp_mls/src/subscriptions/router_callbacks_tests.rs | subscriptions::router_callbacks_tests::transports_key_by_destination | async XMTP; native v3 module; same and different fake host | `MLS-REQ-108` |
| `crates/xmtp_mls/src/subscriptions/mod.rs` | `subscriptions::tests::test_process_streamed_welcome_message` | async XMTP multi-thread; not feature d14n | `MLS-REQ-114` |

## Phase 3 coverage

| File | Qualified test | Form / gates / cases | Requirements |
| --- | --- | --- | --- |
| `crates/xmtp_mls/src/identity_updates.rs` | `identity_updates::conflict_tests::conflict_reloads_validates_and_bounds_identical_resends` | XMTP async; bounded conflict cases and real backend race | `P3-API-011` |
| `crates/xmtp_mls/src/identity_updates.rs` | `identity_updates::conflict_tests::two_clients_racing_identity_updates_keep_both_associations` | XMTP async; bounded conflict cases and real backend race | `P3-API-011` |
| `crates/xmtp_mls/src/client.rs` | `client::tests::registration_visibility_deadline_bounds_a_severed_connection` | Native XMTP async; success first, then disabled proxy; 250 ms wait and 2 s outer bound; proxy restored on panic | `MLS-REQ-045`, `P3-API-015` |
| `crates/xmtp_mls/src/worker/device_sync/tests.rs` | `worker::device_sync::tests::unknown_device_sync_content_is_ignored` | XMTP test; removed protobuf field is ignored | `P3-CFG-005` |
