# MLS client, identity, subscription, and worker test index

[← Test inventory](../existing-tests.md) · [Requirements](../existing-requirements.md)

This index contains 225 source-defined test declarations. All indexed source files are connected to their crate module trees. Listed target and feature gates still apply. A parameterized declaration is one row.

| File | Qualified test | Form / gates / cases | Requirements |
| --- | --- | --- | --- |
| crates/xmtp_mls/src/builder.rs | builder::worker_registration_tests::disabled_worker_is_not_registered | async XMTP test; ignored on wasm | `MLS-REQ-007` |
| crates/xmtp_mls/src/client/tests/groups.rs | client::tests::groups::test_group_member_recovery | async XMTP | `MLS-REQ-008` |
| crates/xmtp_mls/src/client/tests/identity.rs | client::tests::identity::test_client_error_signature_validation_retryability_propagates | sync XMTP | `MLS-REQ-009` |
| crates/xmtp_mls/src/client/tests/identity.rs | client::tests::identity::test_mls_error | async XMTP | `MLS-REQ-010` |
| crates/xmtp_mls/src/client/tests/identity.rs | client::tests::identity::test_register_installation | async XMTP | `MLS-REQ-011` |
| crates/xmtp_mls/src/client/tests/identity.rs | client::tests::identity::test_rotate_key_package | async; wasm-bindgen on wasm; Tokio multi-thread on native | `MLS-REQ-012` |
| crates/xmtp_mls/src/client/tests/groups.rs | client::tests::groups::test_find_groups | async XMTP | `MLS-REQ-013` |
| crates/xmtp_mls/src/client/tests/identity.rs | client::tests::identity::test_find_inbox_id | async XMTP | `MLS-REQ-014` |
| crates/xmtp_mls/src/client/tests/groups.rs | client::tests::groups::test_double_dms | async XMTP | `MLS-REQ-015` |
| crates/xmtp_mls/src/client/tests/sync.rs | client::tests::sync::only_test_sync_welcomes | async rstest and XMTP | `MLS-REQ-016` |
| crates/xmtp_mls/src/client/tests/sync.rs | client::tests::sync::test_leaf_node_lifetime_validation_disabled | async XMTP multi-thread; native only | `MLS-REQ-017` |
| crates/xmtp_mls/src/client/tests/sync.rs | client::tests::sync::test_sync_all_groups | async rstest and XMTP; 10 worker threads | `SHARED-GROUP-REQ-038` |
| crates/xmtp_mls/src/client/tests/sync.rs | client::tests::sync::test_sync_all_groups_and_welcomes | async XMTP multi-thread | `MLS-REQ-019` |
| `crates/xmtp_mls/src/client/tests/sync.rs` | `client::tests::sync::test_sync_100_allowed_groups_performance` | async XMTP plus wasm; creates 100 invites, discards sync count, samples only the first group for one welcome, and asserts no time limit | `SHARED-GROUP-REQ-038` |
| crates/xmtp_mls/src/client/tests/groups.rs | client::tests::groups::test_add_remove_then_add_again | async rstest and XMTP | `SHARED-GROUP-REQ-015` |
| crates/xmtp_mls/src/client/tests/identity.rs | client::tests::identity::test_key_package_rotation | async XMTP; worker timing | `MLS-REQ-012` |
| crates/xmtp_mls/src/client/tests/identity.rs | client::tests::identity::test_find_or_create_dm_by_inbox_id | async XMTP | `MLS-REQ-015` |
| crates/xmtp_mls/src/client/tests/lifecycle.rs | client::tests::lifecycle::should_stream_consent | async XMTP | `SHARED-SYNC-REQ-008` |
| crates/xmtp_mls/src/client/tests/lifecycle.rs | client::tests::lifecycle::should_reconnect | async rstest and XMTP; ignored on wasm; toxiproxy; 100-second cap | `MLS-REQ-023` |
| crates/xmtp_mls/src/client/tests/groups.rs | client::tests::groups::test_list_conversations_pagination | async rstest and XMTP | `SHARED-GROUP-REQ-037` |
| crates/xmtp_mls/src/client/tests/groups.rs | client::tests::groups::test_delete_message | async XMTP | `MLS-REQ-025` |
| crates/xmtp_mls/src/client/tests/lifecycle.rs | client::tests::lifecycle::close_stops_workers | async XMTP | `SHARED-IDENTITY-REQ-019` |
| crates/xmtp_mls/src/client/tests/lifecycle.rs | client::tests::lifecycle::close_is_idempotent | async XMTP | `SHARED-IDENTITY-REQ-019` |
| crates/xmtp_mls/src/client/tests/lifecycle.rs | client::tests::lifecycle::close_disconnects_db | async XMTP; ignored on wasm; persistent database | `SHARED-IDENTITY-REQ-019` |
| crates/xmtp_mls/src/client/tests/lifecycle.rs | client::tests::lifecycle::close_cancels_callback_stream | async XMTP; ignored on wasm | `SHARED-IDENTITY-REQ-019` |
| crates/xmtp_mls/src/client/tests/lifecycle.rs | client::tests::lifecycle::reconnect_after_close_errors | async XMTP | `SHARED-IDENTITY-REQ-019` |
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
| crates/xmtp_mls/src/worker.rs | worker::disconnect_propagation_tests::group_error_forwards_disconnect | sync XMTP; native-only module; group, barrier/storage, published-but-unconfirmed/receiver/processing, and SyncSummary wrappers; benign blocked cause | `MLS-REQ-048` |
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
| crates/xmtp_mls/src/subscriptions/stream_messages/stream_stats.rs | subscriptions::stream_messages::stream_stats::tests::test_stream_stats | async XMTP; disabled workers; one DM and ten groups; bounded wait for Adding, Waiting, and a reconnection covering at least eleven topics | `MLS-REQ-083` |
| crates/xmtp_mls/src/subscriptions/stream_conversations.rs | subscriptions::stream_conversations::test::stream_welcomes | async rstest and XMTP; cases 2 and 5; async fixtures | `SHARED-GROUP-REQ-028` |
| crates/xmtp_mls/src/subscriptions/stream_conversations.rs | subscriptions::stream_conversations::test::test_sync_groups_are_not_streamed | async rstest and XMTP | `SHARED-GROUP-REQ-028` |
| crates/xmtp_mls/src/subscriptions/stream_conversations.rs | subscriptions::stream_conversations::test::test_dm_stream_filter | async rstest and XMTP; DM and Group cases; ignored on wasm | `SHARED-GROUP-REQ-028` |
| crates/xmtp_mls/src/subscriptions/stream_conversations.rs | subscriptions::stream_conversations::test::test_dm_stream_all_conversation_types | async rstest and XMTP | `SHARED-GROUP-REQ-028` |
| crates/xmtp_mls/src/subscriptions/stream_conversations.rs | subscriptions::stream_conversations::test::test_self_group_creation | async rstest and XMTP; 10-second cap | `SHARED-GROUP-REQ-028` |
| `crates/xmtp_mls/src/subscriptions/stream_conversations.rs` | `subscriptions::stream_conversations::test::conversation_consent_filter_preserves_live_baseline` | XMTP async rstest; unfiltered, allowed-only, and empty-filter cases; old consent changes do not replay an existing group | `MLS-REQ-163` |
| crates/xmtp_mls/src/subscriptions/stream_conversations.rs | subscriptions::stream_conversations::test::test_add_remove_re_add | async rstest and XMTP; 5-second cap | `MLS-REQ-086` |
| crates/xmtp_mls/src/subscriptions/stream_conversations.rs | subscriptions::stream_conversations::test::test_duplicate_dm_not_streamed | async rstest and XMTP; 15-second cap | `MLS-REQ-087` |
| `crates/xmtp_mls/src/subscriptions/stream_conversations.rs` | `subscriptions::stream_conversations::test::test_many_concurrent_dm_invites` | async rstest and XMTP; cases 5 and 100 plus wasm; 120 seconds; discards task handles and N stream-poll Option/Result/value outputs | `MLS-REQ-088` |
| crates/xmtp_mls/src/subscriptions/stream_all/tests.rs | subscriptions::stream_all::tests::test_stream_all_messages_changing_group_list | async rstest and XMTP; ignored on wasm | `MLS-REQ-089` |
| crates/xmtp_mls/src/subscriptions/stream_all/tests.rs | subscriptions::stream_all::tests::test_stream_all_messages_unchanging_group_list | async rstest and XMTP | `MLS-REQ-089` |
| crates/xmtp_mls/src/subscriptions/stream_all/tests.rs | subscriptions::stream_all::tests::test_dm_stream_all_messages | async rstest and XMTP | `SHARED-GROUP-REQ-030` |
| `crates/xmtp_mls/src/subscriptions/stream_all/tests.rs` | `subscriptions::stream_all::tests::test_stream_all_messages_does_not_lose_messages` | async rstest and XMTP; existing wasm exclusion; 45 exact application payloads, 16 retained membership rows, unique IDs, and complete recipient history | `MLS-REQ-089` |
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
| crates/xmtp_mls/src/subscriptions/catch_up.rs | subscriptions::catch_up::tests::catch_up_joins_pending_groups_and_stores_history | native XMTP async; backend; two stored payloads, one joined group, at least two stored messages | `SHARED-GROUP-REQ-039` |
| crates/xmtp_mls/src/subscriptions/catch_up.rs | subscriptions::catch_up::tests::catch_up_replays_the_missed_tail_idempotently | native XMTP async; backend; repeated run preserves message count and returns completed with zero new counts | `SHARED-GROUP-REQ-039` |
| crates/xmtp_mls/src/subscriptions/catch_up.rs | subscriptions::catch_up::tests::catch_up_with_nothing_owed_completes | native XMTP async; backend; completed with zero new counts | `SHARED-GROUP-REQ-039` |
| crates/xmtp_mls/src/subscriptions/bidi_tests.rs | subscriptions::bidi_tests::bidi_connection_delivers_live_welcome_over_the_wire | async XMTP; native v3 module; live backend | `MLS-REQ-096` |
| `crates/xmtp_mls/src/subscriptions/bidi_tests.rs` | `subscriptions::bidi_tests::bidi_reaches_applied_target_then_streams_live` | async XMTP; native v3; 5 plus 3 plus 4 messages | `MLS-REQ-097` |
| `crates/xmtp_mls/src/subscriptions/bidi_fuzz_tests.rs` | `subscriptions::bidi_fuzz_tests::fuzz_server_honors_update_acknowledgements_and_targets` | async fuzz-style XMTP; native v3; seed and rounds environment; 300 seconds | `MLS-REQ-102` |
| crates/xmtp_mls/src/subscriptions/bidi_fuzz_tests.rs | subscriptions::bidi_fuzz_tests::fuzz_transport_delivery_never_loses_above_the_floor | async fuzz-style XMTP; native v3; toxiproxy; seed and rounds environment; 300 seconds | `MLS-REQ-103` |
| `crates/xmtp_mls/src/subscriptions/delivery_integration_tests.rs` | `subscriptions::delivery_integration_tests::durable_reader_delivers_live_messages` | native XMTP async; backend; disabled workers; plaintext and backend hash/expiry assertions | `MLS-REQ-104` |
| `crates/xmtp_mls/src/subscriptions/delivery_integration_tests.rs` | `subscriptions::delivery_integration_tests::streamed_message_recovers_pending_rejoin_welcome` | native XMTP async rstest; reader before removal or opened while inactive; group-only scope; disabled workers; no Welcome consumer after fixture join; caller filters application messages | `MLS-REQ-165`, `SHARED-SYNC-REQ-005` |
| `crates/xmtp_mls/src/subscriptions/delivery_integration_tests.rs` | `subscriptions::delivery_integration_tests::durable_reader_delivers_history_from_unary_sync` | native XMTP async; backend; disabled workers; two stored messages read in order | `MLS-REQ-104` |
| `crates/xmtp_mls/src/subscriptions/delivery_integration_tests.rs` | `subscriptions::delivery_integration_tests::acknowledged_message_is_not_redelivered` | native XMTP async; backend; disabled workers; acknowledged read then reader restart | `MLS-REQ-104` |
| `crates/xmtp_mls/src/subscriptions/delivery_integration_tests.rs` | `subscriptions::delivery_integration_tests::sibling_conversation_streams_both_receive_a_welcome` | native XMTP async; backend; two owned conversation streams | `MLS-REQ-105` |
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
| crates/xmtp_mls/src/subscriptions/router_callbacks_tests.rs | subscriptions::router_callbacks_tests::separate_api_clients_at_one_host_use_separate_wires | async XMTP; native v3 module; independent API clients at one host add two transports | `MLS-REQ-108` |
| `crates/xmtp_mls/src/subscriptions/router_callbacks_tests.rs` | `subscriptions::router_callbacks_tests::transport_registry_reuses_only_the_same_api_client` | async XMTP; native v3 module; two registry calls with the same Arc add one transport; another API client adds a second | `MLS-REQ-108` |
| `crates/xmtp_mls/src/subscriptions/mod.rs` | `subscriptions::tests::test_process_streamed_welcome_message` | XMTP async multi-thread; five workers; raw backend welcome envelope | `MLS-REQ-114` |

## Phase 3 coverage

| File | Qualified test | Form / gates / cases | Requirements |
| --- | --- | --- | --- |
| `crates/xmtp_mls/src/identity_updates.rs` | `identity_updates::conflict_tests::conflict_reloads_validates_and_bounds_identical_resends` | XMTP async; bounded conflict cases and real backend race | `P3-API-011` |
| `crates/xmtp_mls/src/identity_updates.rs` | `identity_updates::conflict_tests::two_clients_racing_identity_updates_keep_both_associations` | XMTP async; bounded conflict cases and real backend race | `P3-API-011` |
| `crates/xmtp_mls/src/client/tests/lifecycle.rs` | `client::tests::lifecycle::registration_visibility_deadline_bounds_a_severed_connection` | Native XMTP async; success first, then disabled proxy; 250 ms wait and 2 s outer bound; proxy restored on panic | `MLS-REQ-045`, `P3-API-015` |
| `crates/xmtp_mls/src/worker/device_sync/tests.rs` | `worker::device_sync::tests::unknown_device_sync_content_is_ignored` | XMTP test; removed protobuf field is ignored | `P3-CFG-005` |

## Phase 4 incoming coverage

These rows record source assertions. They do not report a runtime pass.

| File | Qualified test | Form / gates / cases | Requirements |
| --- | --- | --- | --- |
| `crates/xmtp_mls/src/subscriptions/incoming/controller/tests.rs` | `subscriptions::incoming::controller::tests::a_second_scope_captures_a_fresh_target_on_the_shared_registration` | XMTP async; mocked newest query; independent targets 40 and 90 | `MLS-REQ-116` |
| `crates/xmtp_mls/src/subscriptions/incoming/controller/tests.rs` | `subscriptions::incoming::controller::tests::a_replaced_scope_ignores_an_older_target_request` | XMTP sync; stale generation reply | `MLS-REQ-117` |
| `crates/xmtp_mls/src/subscriptions/incoming/controller/tests.rs` | `subscriptions::incoming::controller::tests::permanent_source_and_topic_errors_stop_automatic_receipt` | XMTP sync; capacity and protocol errors | `MLS-REQ-118` |
| `crates/xmtp_mls/src/subscriptions/incoming/controller/tests.rs` | `subscriptions::incoming::controller::tests::receipt_acknowledgement_follows_storage_and_never_uses_the_target` | XMTP async; real storage; receipt 20, processed 0, target 90; duplicate admission | `MLS-REQ-119` |
| `crates/xmtp_mls/src/subscriptions/incoming/controller/tests.rs` | `subscriptions::incoming::controller::tests::replacement_cancels_old_obligations_and_retains_notifications` | XMTP async; cancelled prior scope and retained notification | `MLS-REQ-117` |
| `crates/xmtp_mls/src/subscriptions/incoming/controller/tests.rs` | `subscriptions::incoming::controller::tests::an_invalid_supported_head_does_not_hold_a_later_valid_message` | XMTP async; malformed bytes; tampered unconsumed ciphertext; valid original; reused generation; later commit and message | `MLS-REQ-120` |
| `crates/xmtp_mls/src/groups/mls_sync/processing_policy.rs` | `groups::mls_sync::processing_policy::tests::obsolete_generations_are_rejected_but_local_secret_tree_failures_stay_pending` | XMTP sync; nested secret-tree error classification | `MLS-REQ-120` |
| `crates/xmtp_mls/src/identity_updates/dependencies.rs` | `identity_updates::dependencies::tests::identity_and_query_futures_are_send` | native XMTP sync; compile-time Send bounds for mock and concrete client futures | `MLS-REQ-121` |
| `crates/xmtp_mls/src/identity_updates/dependencies.rs` | `identity_updates::dependencies::tests::identity_requests_share_one_context_limit` | XMTP async; one held and released permit | `MLS-REQ-122` |
| `crates/xmtp_mls/src/identity_updates/dependencies.rs` | `identity_updates::dependencies::tests::concurrent_requirements_share_one_attempt_but_later_calls_retry` | XMTP async; two shared holders then a fresh later attempt | `MLS-REQ-123` |
| `crates/xmtp_mls/src/identity_updates/dependencies.rs` | `identity_updates::dependencies::tests::historical_proof_does_not_use_a_newer_cached_snapshot` | XMTP async; backend; original and added installations | `MLS-REQ-124` |
| `crates/xmtp_mls/src/identity_updates/dependencies.rs` | `identity_updates::dependencies::tests::dependency_batch_keeps_success_after_an_invalid_sibling` | XMTP async; backend; duplicate valid input and invalid sequence zero | `MLS-REQ-125` |
| `crates/xmtp_mls/src/identity_updates/dependencies.rs` | `identity_updates::dependencies::tests::primary_absence_is_terminal_only_after_successful_query` | XMTP async; backend; absent inbox; zero retry wait; no injected transport failure | `MLS-REQ-126` |
| `crates/xmtp_mls/src/subscriptions/incoming/controller/tests.rs` | `subscriptions::incoming::controller::tests::a_barrier_keeps_its_fixed_target_across_registration_and_target_replies` | XMTP async; disabled workers; targets 40, 90, and 120; replacement 60; later registration 150 and query 180 | `MLS-REQ-127` |
| `crates/xmtp_mls/src/subscriptions/incoming/controller/tests.rs` | `subscriptions::incoming::controller::tests::a_new_barrier_after_an_empty_query_starts_deadline_fallback` | XMTP async; disabled workers; recent empty query; one in-flight read; active poll retries; receipt-target stop | `MLS-REQ-128` |
| `crates/xmtp_mls/src/subscriptions/incoming/controller/tests.rs` | `subscriptions::incoming::controller::tests::a_healthy_receiver_gets_one_fixed_barrier_wait` | XMTP async; disabled workers; barrier and live scope; partial receipt keeps original wait; caught-up healthy receipt stops unary polling | `MLS-REQ-128` |
| `crates/xmtp_mls/src/subscriptions/incoming/controller/tests.rs` | `subscriptions::incoming::controller::tests::a_welcome_failure_keeps_independent_pending_parents_runnable` | XMTP async; disabled workers; identity and source failures; independent Welcome completion leaves one blocked parent | `MLS-REQ-129` |
| `crates/xmtp_mls/src/subscriptions/incoming/controller/tests.rs` | `subscriptions::incoming::controller::tests::reopening_after_the_last_release_keeps_the_controller_alive` | XMTP sync; retained handle across release/reacquire; final shutdown clears cached owner and closes commands | `MLS-REQ-130` |
| `crates/xmtp_mls/src/subscriptions/barrier/tests.rs` | `subscriptions::barrier::tests::fixed_welcome_discovery_excludes_later_scope_and_keeps_rejoined_groups` | XMTP integration test; real tester storage; fixed discovery target and later rejoin/local creation | `MLS-REQ-131` |
| `crates/xmtp_mls/src/subscriptions/barrier/tests.rs` | `subscriptions::barrier::tests::a_stalled_welcome_does_not_hold_known_group_processing` | XMTP integration test; known group completes while Welcome and target-capture obligations remain unfinished | `MLS-REQ-132` |
| `crates/xmtp_mls/src/subscriptions/barrier/tests.rs` | `subscriptions::barrier::tests::target_capture_timeout_reports_every_starting_topic` | XMTP integration test; exhausted deadline; every requested topic retains an absent target and pending cause | `MLS-REQ-133` |
| `crates/xmtp_mls/src/subscriptions/incoming/controller/tests.rs` | `subscriptions::incoming::controller::tests::each_kind_keeps_its_budget_and_only_committed_chunks_are_acknowledged` | XMTP async; registered tester storage; per-kind pending limits and one-row admission; committed-prefix acknowledgements and resumed overlap | `MLS-REQ-134` |
| `crates/xmtp_mls/src/subscriptions/incoming/controller/tests.rs` | `subscriptions::incoming::controller::tests::byte_chunks_validate_the_complete_input_before_receipt` | XMTP async; registered tester storage; one-envelope byte limit; invalid whole input leaves receipt unchanged | `MLS-REQ-135` |
| `crates/xmtp_mls/src/subscriptions/local_delivery/tests.rs` | `subscriptions::local_delivery::tests::iterator_drop_retains_the_last_item_until_a_later_next_request` | XMTP async; registered tester storage; iterator drop/reopen; next-request acknowledgement | `MLS-REQ-138` |
| `crates/xmtp_mls/src/subscriptions/local_delivery/tests.rs` | `subscriptions::local_delivery::tests::rejected_callback_releases_owner_without_consuming_its_item` | XMTP async; registered tester storage; explicit rejection; busy-owner fence; replacement reader | `MLS-REQ-139` |
| `crates/xmtp_mls/src/subscriptions/local_delivery/tests.rs` | `subscriptions::local_delivery::tests::cancelling_a_pending_next_does_not_bypass_explicit_acknowledgement` | XMTP async; registered tester storage; cancelled next polls; explicit acknowledgement | `MLS-REQ-140` |
| `crates/xmtp_mls/src/subscriptions/local_delivery/tests.rs` | `subscriptions::local_delivery::tests::excluded_rows_stay_consumed_after_a_filter_change` | XMTP async; registered tester storage; consent filter; stale selection revision; STR-092 scope preservation and STR-093 filtered advance | `MLS-REQ-141` |
| `crates/xmtp_mls/src/subscriptions/local_delivery/tests.rs` | `subscriptions::local_delivery::tests::missed_local_wake_is_recovered_by_a_fresh_database_poll` | XMTP async; registered tester storage; direct insert without wake; 20 ms polling; 2 s outer bound | `MLS-REQ-142` |
| `crates/xmtp_mls/src/subscriptions/local_delivery/tests.rs` | `subscriptions::local_delivery::tests::stale_host_queue_token_cannot_dispatch_or_acknowledge` | XMTP async; registered tester storage; database identity rotation; stale handoff and acknowledgement | `MLS-REQ-143` |
| `crates/xmtp_mls/src/subscriptions/local_delivery/tests.rs` | `subscriptions::local_delivery::tests::removed_and_readded_scope_discards_old_queued_tokens_without_acknowledging` | XMTP async; registered tester storage; scope revision invalidation; retained row; already-started callback | `MLS-REQ-144` |
| `crates/xmtp_mls/src/subscriptions/local_delivery/tests.rs` | `subscriptions::local_delivery::tests::queued_content_is_rechecked_after_deletion_and_restore` | XMTP async; registered tester storage; explicit replay; deletion before handoff; ForeignCursor after restore | `MLS-REQ-145` |
| `crates/xmtp_mls/src/subscriptions/incoming/controller/tests.rs` | `subscriptions::incoming::controller::tests::suspension_blocks_live_queries_but_allows_an_explicit_barrier` | XMTP async; suspended transport; no live read or target query; explicit missing prefix only | `MLS-REQ-146` |
| `crates/xmtp_mls/src/subscriptions/stream_failure/tests.rs` | `subscriptions::stream_failure::tests::preserves_missing_targets_and_full_width_cursors` | XMTP sync; JSON round trip; null target and full-width decimal cursors | `MLS-REQ-148` |
| `crates/xmtp_mls/src/subscriptions/stream_failure/tests.rs` | `subscriptions::stream_failure::tests::published_failure_keeps_zero_target_and_intent_identity` | XMTP sync; cancelled barrier; present zero target; retained intent identity | `MLS-REQ-149` |
| `crates/xmtp_mls/src/subscriptions/stream_failure/tests.rs` | `subscriptions::stream_failure::tests::published_failure_without_barrier_keeps_intent_identity` | XMTP sync; absent barrier; retained intent identity | `MLS-REQ-149` |
| `crates/xmtp_mls/src/subscriptions/stream_failure/tests.rs` | `subscriptions::stream_failure::tests::traverses_all_sync_summary_branches_and_transparent_wrappers` | XMTP sync; four nested barrier paths; every published intent; no invented primary intent | `MLS-REQ-150` |
| `crates/xmtp_mls/src/subscriptions/stream_failure/tests.rs` | `subscriptions::stream_failure::tests::catch_up_failure_keeps_partial_summary_and_all_barriers` | Native XMTP sync; incomplete summary; exact large counters; two barrier causes | `MLS-REQ-151` |
| `crates/xmtp_mls/src/subscriptions/stream_failure/tests.rs` | `subscriptions::stream_failure::tests::details_do_not_include_storage_error_data` | XMTP sync; private storage error data replaced by fixed code and message | `MLS-REQ-152` |
| `crates/xmtp_mls/src/subscriptions/stream_failure/tests.rs` | `subscriptions::stream_failure::tests::ordinary_and_invalid_errors_have_no_details` | XMTP sync; ordinary error; ordinary text; invalid JSON suffix | `MLS-REQ-153` |
| `crates/xmtp_mls/src/client/tests/lifecycle.rs` | `client::tests::lifecycle::registration_visibility_waits_for_serving_head` | XMTP async rstest; two cases; mocked empty, older, then equal/newer serving head; 1 s deadline | `MLS-REQ-045` |
| `crates/xmtp_mls/src/client/tests/lifecycle.rs` | `client::tests::lifecycle::registration_visibility_rejects_mismatched_metadata` | XMTP async; mocked mismatched metadata topic; one request; typed invalid response | `MLS-REQ-045` |
| `crates/xmtp_mls/src/worker/device_sync/archive.rs` | `worker::device_sync::archive::tests::archive_timestamp_keeps_a_message_received_during_import` | XMTP async; stale import context; later optimistic message; current timestamp preserved | `MLS-REQ-057` |
| `crates/xmtp_mls/src/worker/key_package_maintenance.rs` | `worker::key_package_maintenance::tests::pending_welcome_preserves_expired_keys_until_completion` | Native XMTP async; real key package; forced expiry; pending Welcome; before/after completion sweep | `MLS-REQ-157` |
| `crates/xmtp_mls/src/subscriptions/incoming/controller/tests.rs` | `subscriptions::incoming::controller::tests::suspended_welcome_barrier_receives_only_its_required_group_prefixes` | XMTP async; suspended factory; fixed Welcome target; parent cancellation; partial and complete prefix receipt; later parents stay paused | `MLS-REQ-161` |
| `crates/xmtp_mls/src/subscriptions/incoming/controller/tests.rs` | `subscriptions::incoming::controller::tests::welcome_prefix_fallback_keeps_the_parent_barrier_deadline` | XMTP async; covered prefix topic; long and urgent parent deadlines; fixed receiver wait and database poll spacing | `MLS-REQ-162` |
| `crates/xmtp_mls/src/subscriptions/policy.rs` | `subscriptions::policy::tests::internal_limits_fit_storage_and_timer_ranges` | XMTP unit; default internal policy bounds | `MLS-REQ-166` |
| `crates/xmtp_mls/src/subscriptions/incoming/controller/tests.rs` | `subscriptions::incoming::controller::tests::client_setup_selects_the_platform_transport_without_starting_it` | XMTP async; native and browser; empty interest; two readers; endpoint counts | `MLS-REQ-167` |
| `crates/xmtp_mls/src/subscriptions/incoming/controller/tests.rs` | `subscriptions::incoming::controller::tests::releasing_readers_allows_recreation_and_releases_the_context` | XMTP async; two reader lifetimes; weak context after shutdown | `MLS-REQ-168` |
| `crates/xmtp_mls/src/subscriptions/incoming/controller/tests.rs` | `subscriptions::incoming::controller::tests::replacing_the_api_with_query_only_setup_receives_and_processes_messages` | XMTP async; rebuilt client; peer message; endpoint counts | `MLS-REQ-169` |
| `crates/xmtp_mls/src/subscriptions/incoming/controller/tests.rs` | `subscriptions::incoming::controller::tests::stale_dependency_results_cannot_change_new_heads_or_requirements` | XMTP async; group, identity, and Welcome parents; success, missing proof, and terminal failure | `MLS-REQ-170` |
| `crates/xmtp_mls/src/subscriptions/incoming/controller/tests.rs` | `subscriptions::incoming::controller::tests::passive_prefixes_do_not_limit_shared_identity_requests` | XMTP async; full passive watch count; shared group and Welcome request | `MLS-REQ-171` |
| `crates/xmtp_mls/src/subscriptions/delivery_integration_tests.rs` | `subscriptions::delivery_integration_tests::the_same_reader_recovers_a_missed_commit_after_a_tcp_outage` | XMTP async; native; private TCP proxy; same reader; no receiver sync | `MLS-REQ-172` |
