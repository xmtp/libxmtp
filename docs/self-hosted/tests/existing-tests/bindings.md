# Mobile, Node, and WebAssembly binding test index

[← Test inventory](../existing-tests.md) · [Requirements](../existing-requirements.md)

### Mobile bindings

| File | Qualified test name | Form, gates, and cases | Requirements |
| --- | --- | --- | --- |
| `bindings/mobile/examples/ExampleInstrumentedTest.kt` | `ExampleInstrumentedTest.testHappyPath` | JUnit4 Android instrumentation; emulator and local backend; example has no local Gradle runner | `BIND-REQ-010` |
| `bindings/mobile/examples/ExampleInstrumentedTest.kt` | `ExampleInstrumentedTest.testErrorThrows` | JUnit4 Android instrumentation; bad host; example has no local Gradle runner | `BIND-REQ-024` |
| `bindings/mobile/src/builder_test.rs` | `test_primitive_constructor_and_setters` | Rust unit; file gated by cfg(test) | `BIND-REQ-001` |
| `bindings/mobile/src/builder_test.rs` | `test_setter_chaining` | Rust unit; cfg(test) | `BIND-REQ-001` |
| `bindings/mobile/src/builder_test.rs` | `test_defaults_applied_and_overridden` | Rust unit; cfg(test) | `BIND-REQ-001` |
| `bindings/mobile/src/builder_test.rs` | `test_skip_fields_initialized` | Rust unit; cfg(test) | `BIND-REQ-001` |
| `bindings/mobile/src/builder_test.rs` | `test_mixed_modes` | Rust unit; cfg(test) | `BIND-REQ-001` |
| `bindings/mobile/src/builder_test.rs` | `test_mixed_chaining` | Rust unit; cfg(test) | `BIND-REQ-001` |
| `bindings/mobile/src/crypto.rs` | `tests::test_ffi_error_mapping_invalid_private_key_length` | Rust unit; cfg(test); 31- and 33-byte key and signing cases | `BIND-REQ-003` |
| `bindings/mobile/src/crypto.rs` | `tests::test_ffi_error_mapping_invalid_private_key` | Rust unit; cfg(test); zero key for public key and signing | `BIND-REQ-003` |
| `bindings/mobile/src/crypto.rs` | `tests::test_ffi_error_mapping_invalid_pubkey_length` | Rust unit; cfg(test); 32 and 100 bytes and wrong prefix | `BIND-REQ-003` |
| `bindings/mobile/src/crypto.rs` | `tests::test_ffi_error_mapping_invalid_hash_length` | Rust unit; cfg(test); 31- and 33-byte prehash | `BIND-REQ-003` |
| `bindings/mobile/src/crypto.rs` | `tests::test_ffi_basic_functionality` | Rust unit; cfg(test); known valid key | `BIND-REQ-004` |
| `bindings/mobile/src/lib.rs` | `lib_tests::test_get_version_info` | Rust unit; cfg(test); prints without assertion | `BIND-REQ-099` |
| `bindings/mobile/src/lib.rs` | `lib_tests::test_generic_error_code_unit_variant` | Rust unit; cfg(test) | `BIND-REQ-005` |
| `bindings/mobile/src/lib.rs` | `lib_tests::test_generic_error_code_string_variant` | Rust unit; cfg(test) | `BIND-REQ-005` |
| `bindings/mobile/src/lib.rs` | `lib_tests::test_generic_error_code_inherited_storage` | Rust unit; cfg(test) | `BIND-REQ-005` |
| `bindings/mobile/src/lib.rs` | `lib_tests::test_generic_error_code_expired` | Rust unit; cfg(test) | `BIND-REQ-005` |
| `bindings/mobile/src/lib.rs` | `lib_tests::test_ffi_error_display_format` | Rust unit; cfg(test) | `BIND-REQ-005` |
| `bindings/mobile/src/lib.rs` | `lib_tests::test_ffi_error_display_inherited_code` | Rust unit; cfg(test) | `BIND-REQ-005` |
| `bindings/mobile/src/lib.rs` | `lib_tests::test_ffi_error_source` | Rust unit; cfg(test) | `BIND-REQ-005` |
| `bindings/mobile/src/lib.rs` | `lib_tests::test_generic_error_from_string` | Rust unit; cfg(test) | `BIND-REQ-005` |
| `bindings/mobile/src/lib.rs` | `lib_tests::test_generic_error_from_error` | Rust unit; cfg(test) | `BIND-REQ-005` |
| `bindings/mobile/src/lib.rs` | `lib_tests::test_ffi_error_from_expired` | Rust unit; cfg(test) | `BIND-REQ-005` |
| `bindings/mobile/src/lib.rs` | `lib_tests::test_ffi_error_from_various_error_types` | Rust unit; cfg(test) | `BIND-REQ-005` |
| `bindings/mobile/src/logger.rs` | `sentry_tests::ffi_config_maps_and_bad_dsn_errors` | Rust unit; cfg(test); invalid-DSN or host-owned subscriber branch | `BIND-REQ-008` |
| `bindings/mobile/src/logger.rs` | `test_logger::test_file_appender` | Rust unit; cfg(test); temporary filesystem and global logger | `BIND-REQ-009` |
| `bindings/mobile/tests/telemetry_init.rs` | `flush_and_disable_before_init_touch_nothing` | Rust integration; cfg(not(wasm32)); fresh process | `BIND-REQ-008` |
| `bindings/mobile/src/mls/device_sync/mod.rs` | `unit_tests::test_check_key_too_short` | Rust unit; cfg(test); 31 bytes | `BIND-REQ-070` |
| `bindings/mobile/src/mls/device_sync/mod.rs` | `unit_tests::test_check_key_exact_length` | Rust unit; cfg(test); 32 bytes | `BIND-REQ-070` |
| `bindings/mobile/src/mls/device_sync/mod.rs` | `unit_tests::test_check_key_longer_gets_truncated` | Rust unit; cfg(test); 64 sequential bytes | `BIND-REQ-070` |
| `bindings/mobile/src/mls/device_sync/mod.rs` | `unit_tests::test_check_key_empty` | Rust unit; cfg(test); empty | `BIND-REQ-070` |
| `bindings/mobile/src/mls/device_sync/mod.rs` | `unit_tests::test_ffi_backup_element_selection_to_backup_element_selection` | Rust unit; Messages and Consent | `BIND-REQ-071` |
| `bindings/mobile/src/mls/device_sync/mod.rs` | `unit_tests::test_backup_element_selection_to_ffi_backup_element_selection` | Rust unit; Messages and Consent | `BIND-REQ-071` |
| `bindings/mobile/src/mls/device_sync/mod.rs` | `unit_tests::test_backup_element_selection_unspecified_fails` | Rust unit; Unspecified | `BIND-REQ-071` |
| `bindings/mobile/src/mls/device_sync/mod.rs` | `unit_tests::test_proto_backup_element_selection_to_ffi_backup_element_selection` | Rust unit; proto Messages and Consent | `BIND-REQ-071` |
| `bindings/mobile/src/mls/device_sync/mod.rs` | `unit_tests::test_proto_backup_element_selection_unspecified_fails` | Rust unit; proto Unspecified | `BIND-REQ-071` |
| `bindings/mobile/src/mls/device_sync/mod.rs` | `unit_tests::test_ffi_archive_options_to_backup_options` | Rust unit; full range, elements, and exclusion | `BIND-REQ-072` |
| `bindings/mobile/src/mls/device_sync/mod.rs` | `unit_tests::test_ffi_archive_options_empty_elements` | Rust unit; empty and default options | `BIND-REQ-072` |
| `bindings/mobile/src/mls/device_sync/mod.rs` | `unit_tests::test_backup_metadata_to_ffi_backup_metadata` | Rust unit; full metadata | `BIND-REQ-073` |
| `bindings/mobile/src/mls/device_sync/mod.rs` | `unit_tests::test_backup_metadata_filters_unspecified_elements` | Rust unit; supported and Unspecified | `BIND-REQ-073` |
| `bindings/mobile/src/mls/device_sync/mod.rs` | `unit_tests::test_available_archive_to_ffi_available_archive` | Rust unit; PIN, sender, and nested metadata | `BIND-REQ-073` |
| `bindings/mobile/src/mls/device_sync/tests.rs` | `test_create_new_installation_without_breaking_group` | Tokio multi-thread, 5 workers; local backend | `BIND-REQ-064`, `BIND-REQ-042` |
| `bindings/mobile/src/mls/device_sync/tests.rs` | `test_create_new_installations_does_not_fork_group` | Tokio multi-thread, 5 workers; streams and second Bo installation | `BIND-REQ-064`, `BIND-REQ-055` |
| `bindings/mobile/src/mls/device_sync/tests.rs` | `test_can_sync_all_groups` | Tokio multi-thread, 5 workers; 30 groups; ignored with d14n | `SHARED-GROUP-REQ-038` |
| `bindings/mobile/src/mls/device_sync/tests.rs` | `test_can_sync_all_groups_active_only` | Tokio multi-thread, 5 workers; 30 groups; ignored with d14n | `SHARED-GROUP-REQ-038`, `SHARED-GROUP-REQ-013` |
| `bindings/mobile/src/mls/device_sync/tests.rs` | `test_can_send_message_when_out_of_sync` | Tokio multi-thread, 5 workers; stale by 3 epochs | `BIND-REQ-063` |
| `bindings/mobile/src/mls/device_sync/tests.rs` | `test_can_send_messages_when_epochs_behind` | Tokio multi-thread, 5 workers; stale by 4 metadata epochs | `BIND-REQ-063` |
| `bindings/mobile/src/mls/device_sync/tests.rs` | `test_can_add_members_when_out_of_sync` | Tokio multi-thread, 5 workers; stale by 3 epochs | `BIND-REQ-063`, `SHARED-GROUP-REQ-011` |
| `bindings/mobile/src/mls/device_sync/tests.rs` | `test_revoke_installation_for_two_users_and_group_modification` | Tokio multi-thread, 5 workers; two-member group | `BIND-REQ-065`, `SHARED-IDENTITY-REQ-006` |
| `bindings/mobile/src/mls/device_sync/tests.rs` | `test_revoke_installation_for_one_user_and_group_modification` | Tokio multi-thread, 5 workers; solo group | `BIND-REQ-065`, `SHARED-IDENTITY-REQ-006` |
| `bindings/mobile/src/mls/device_sync/tests.rs` | `test_send_sync_request_flow` | Tokio async; sync workers and local archive URL | `SHARED-SYNC-REQ-007` |
| `bindings/mobile/src/mls/device_sync/tests.rs` | `test_new_installation_group_message_visibility` | Tokio async; PIN archive and messages before or after install | `SHARED-SYNC-REQ-007`, `BIND-REQ-042` |
| `bindings/mobile/src/mls/device_sync/tests.rs` | `test_sync_consent` | Tokio async; two installations and sync groups | `BIND-REQ-067`, `SHARED-GROUP-REQ-021` |
| `bindings/mobile/src/mls/device_sync/tests.rs` | `test_set_and_get_group_consent` | Tokio multi-thread, 5 workers; creator and invitee | `SHARED-GROUP-REQ-021` |
| `bindings/mobile/src/mls/device_sync/tests.rs` | `test_set_and_get_member_consent` | Tokio multi-thread, 5 workers; inbox entity and member projection | `SHARED-GROUP-REQ-022` |
| `bindings/mobile/src/mls/tests/client.rs` | `test_create_client_with_storage` | Tokio multi-thread, 1 worker; same database reopened | `BIND-REQ-010` |
| `bindings/mobile/src/mls/tests/client.rs` | `test_create_client_with_key` | Tokio multi-thread, 1 worker; correct then altered key | `BIND-REQ-010` |
| `bindings/mobile/src/mls/tests/client.rs` | `test_can_message` | Tokio multi-thread, 1 worker; before and after peer registration | `SHARED-IDENTITY-REQ-001` |
| `bindings/mobile/src/mls/tests/client.rs` | `test_key_package_validation` | Tokio multi-thread, 5 workers; nonempty installation list and equal status-map cardinality; lifetime assertions run only without validation error and with a present lifetime | `SHARED-IDENTITY-REQ-008` |
| `bindings/mobile/src/mls/tests/client.rs` | `test_get_hmac_keys` | Tokio multi-thread, 5 workers; one group; three nonempty 42-byte keys with epochs at least 1 | `SHARED-GROUP-REQ-041` |
| `bindings/mobile/src/mls/tests/client.rs` | `test_shutdown_is_idempotent` | Tokio multi-thread, 1 worker; two calls | `SHARED-IDENTITY-REQ-019` |
| `bindings/mobile/src/mls/tests/identity.rs` | `get_inbox_id` | Tokio async; network lookup | `SHARED-IDENTITY-REQ-001` |
| `bindings/mobile/src/mls/tests/identity.rs` | `test_can_add_wallet_to_inbox` | Tokio multi-thread, 1 worker; second ECDSA wallet | `SHARED-IDENTITY-REQ-003` |
| `bindings/mobile/src/mls/tests/identity.rs` | `test_associate_passkey` | Tokio multi-thread, 1 worker; passkey signature | `SHARED-IDENTITY-REQ-003` |
| `bindings/mobile/src/mls/tests/identity.rs` | `test_can_revoke_wallet` | Tokio multi-thread, 1 worker; add then recovery-signed revoke | `SHARED-IDENTITY-REQ-003` |
| `bindings/mobile/src/mls/tests/identity.rs` | `test_invalid_external_signature` | Tokio multi-thread, 1 worker; missing external signature | `BIND-REQ-015` |
| `bindings/mobile/src/mls/tests/identity.rs` | `test_sign_and_verify` | Tokio multi-thread, 5 workers; changed text and zero signature | `SHARED-IDENTITY-REQ-005` |
| `bindings/mobile/src/mls/tests/identity.rs` | `test_revoke_all_installations` | Tokio multi-thread, 5 workers; two installations | `SHARED-IDENTITY-REQ-006` |
| `bindings/mobile/src/mls/tests/identity.rs` | `test_revoke_all_installations_no_crash` | Tokio multi-thread, 5 workers; singleton no-op then two-install revoke | `SHARED-IDENTITY-REQ-006` |
| `bindings/mobile/src/mls/tests/identity.rs` | `test_revoke_installations` | Tokio multi-thread, 5 workers; selected second installation | `SHARED-IDENTITY-REQ-006` |
| `bindings/mobile/src/mls/tests/identity.rs` | `test_can_not_create_new_inbox_id_with_already_associated_wallet` | Tokio multi-thread, 5 workers; wallet B joins inbox A, messaging and mismatched inbox B | `BIND-REQ-019` |
| `bindings/mobile/src/mls/tests/identity.rs` | `test_wallet_b_cannot_create_new_client_for_inbox_b_after_association` | Tokio multi-thread, 5 workers; existing inbox B then association to A | `BIND-REQ-019` |
| `bindings/mobile/src/mls/tests/identity.rs` | `test_cannot_create_more_than_max_installations` | Tokio async; cap, revoke, and replacement | `SHARED-IDENTITY-REQ-007` |
| `bindings/mobile/src/mls/tests/identity.rs` | `test_sorts_members_by_created_at_using_ffi_identifiers` | Tokio multi-thread, 1 worker; five added wallets | `BIND-REQ-020` |
| `bindings/mobile/src/mls/tests/static_methods.rs` | `test_static_revoke_installations` | Tokio multi-thread, 5 workers; five installations | `SHARED-IDENTITY-REQ-006` |
| `bindings/mobile/src/mls/tests/static_methods.rs` | `test_static_revoke_fails_with_non_recovery_identity` | Tokio multi-thread, 5 workers; ignored with d14n | `SHARED-IDENTITY-REQ-006` |
| `bindings/mobile/src/mls/tests/static_methods.rs` | `test_can_get_inbox_state_statically` | Tokio async; three installations | `SHARED-IDENTITY-REQ-002` |
| `bindings/mobile/src/mls/tests/static_methods.rs` | `test_get_newest_message_metadata` | Tokio multi-thread, 2 workers; two messages and empty group | `BIND-REQ-096` |
| `bindings/mobile/src/mls/tests/networking.rs` | `radio_silence` | Tokio multi-thread, 1 worker; sync worker, stream, and 5-second idle | `BIND-REQ-023` |
| `bindings/mobile/src/mls/tests/networking.rs` | `create_client_does_not_hit_network` | Tokio multi-thread, 1 worker; registration then existing-database open | `BIND-REQ-023`, `BIND-REQ-010` |
| `bindings/mobile/src/mls/tests/networking.rs` | `ffi_api_stats_exposed_correctly` | Tokio multi-thread, 1 worker; create, clear, and create | `BIND-REQ-023` |
| `bindings/mobile/src/mls/tests/networking.rs` | `test_is_connected_after_connect` | Tokio async; good and unreachable endpoints | `BIND-REQ-024` |
| `bindings/mobile/src/mls/tests/lifecycle.rs` | `bidi_suspend_and_resume_redelivers` | Tokio multi-thread, 5 workers; bidi env on; ignored with d14n; nextest isolation | `BIND-REQ-060` |
| `bindings/mobile/src/mls/tests/lifecycle.rs` | `bidi_catch_up_to_live_replays_and_is_idempotent` | Tokio multi-thread, 5 workers; bidi env on; two runs | `BIND-REQ-061` |
| `bindings/mobile/src/mls/tests/lifecycle.rs` | `bidi_catch_up_to_live_bounded_run_is_cancel_safe` | Tokio multi-thread, 5 workers; bidi env on; 1 ms, full, and drained runs | `BIND-REQ-061` |
| `bindings/mobile/src/mls/tests/lifecycle.rs` | `catch_up_to_live_falls_back_when_bidi_disabled` | Tokio multi-thread, 5 workers; bidi env unset | `BIND-REQ-061` |
| `bindings/mobile/src/mls/tests/archive.rs` | `test_archive_excludes_disappearing_messages` | Tokio multi-thread, 5 workers; two exports and imports | `BIND-REQ-068` |
| `bindings/mobile/src/mls/tests/dms.rs` | `test_find_or_create_dm` | Tokio async; repeated and opposite-side calls | `SHARED-GROUP-REQ-001` |
| `bindings/mobile/src/mls/tests/dms.rs` | `test_dms_sync_but_do_not_list` | Tokio multi-thread, 5 workers; DM versus group filters and summaries | `SHARED-GROUP-REQ-001`, `SHARED-GROUP-REQ-038` |
| `bindings/mobile/src/mls/tests/dms.rs` | `test_dm_stream_correct_type` | Tokio multi-thread, 5 workers; DM-only stream | `SHARED-GROUP-REQ-028`, `SHARED-GROUP-REQ-006` |
| `bindings/mobile/src/mls/tests/dms.rs` | `test_dm_streaming` | Tokio multi-thread, 5 workers; all, Group, and DM conversation streams | `SHARED-GROUP-REQ-028` |
| `bindings/mobile/src/mls/tests/dms.rs` | `test_stream_all_dm_messages` | Tokio multi-thread, 5 workers; all, Group, and DM message streams; ignored with d14n | `BIND-REQ-055` |
| `bindings/mobile/src/mls/tests/dms.rs` | `test_dm_first_messages` | Tokio multi-thread, 5 workers; DM and group histories | `SHARED-GROUP-REQ-026` |
| `bindings/mobile/src/mls/tests/dms.rs` | `test_get_dm_peer_inbox_id` | Tokio multi-thread, 5 workers; both participant views | `SHARED-GROUP-REQ-006` |
| `bindings/mobile/src/mls/tests/dms.rs` | `test_dm_permissions_show_expected_values` | Tokio multi-thread, 5 workers; DM versus default group | `SHARED-GROUP-REQ-018` |
| `bindings/mobile/src/mls/tests/dms.rs` | `test_set_disappearing_messages_when_creating_dm` | Tokio multi-thread, 5 workers; 2-second expiry | `SHARED-GROUP-REQ-025` |
| `bindings/mobile/src/mls/tests/dms.rs` | `test_can_successfully_thread_dms` | Tokio async; independent DMs and bidirectional messages | `SHARED-GROUP-REQ-002`, `SHARED-GROUP-REQ-026` |
| `bindings/mobile/src/mls/tests/dms.rs` | `test_can_successfully_thread_dms_with_no_messages` | Tokio async; independent empty DMs | `SHARED-GROUP-REQ-002` |
| `bindings/mobile/src/mls/tests/dms.rs` | `test_can_quickly_fetch_dm_peer_inbox_id` | Tokio async; direct, list, and stream handles | `SHARED-GROUP-REQ-006`, `SHARED-GROUP-REQ-028` |
| `bindings/mobile/src/mls/tests/dms.rs` | `test_create_new_installation_can_see_dm` | Tokio multi-thread, 5 workers; same-wallet second installation | `BIND-REQ-042` |
| `bindings/mobile/src/mls/tests/dms.rs` | `test_can_find_duplicate_dms_for_group` | Tokio async; two physical DMs | `SHARED-GROUP-REQ-002` |
| `bindings/mobile/src/mls/tests/dms.rs` | `test_set_and_get_dm_consent` | Tokio multi-thread, 5 workers; creator and invitee | `SHARED-GROUP-REQ-021` |
| `bindings/mobile/src/mls/tests/group_management.rs` | `test_create_group_with_members` | Tokio multi-thread, 1 worker; one invitee | `SHARED-GROUP-REQ-007` |
| `bindings/mobile/src/mls/tests/group_management.rs` | `test_create_group_with_metadata` | Tokio multi-thread, 1 worker; all metadata and disappearing settings 10/100 | `SHARED-GROUP-REQ-008` |
| `bindings/mobile/src/mls/tests/group_management.rs` | `test_removed_members_no_longer_update` | Tokio multi-thread, 5 workers; remove then send | `SHARED-GROUP-REQ-013` |
| `bindings/mobile/src/mls/tests/group_management.rs` | `test_group_permissions_show_expected_values` | Tokio multi-thread, 5 workers; admin-only and default | `SHARED-GROUP-REQ-018` |
| `bindings/mobile/src/mls/tests/group_management.rs` | `test_permissions_updates` | Tokio multi-thread, 5 workers; image allowed and name denied | `SHARED-GROUP-REQ-020` |
| `bindings/mobile/src/mls/tests/group_management.rs` | `test_app_data_permission_update` | Tokio multi-thread, 5 workers; Admin to Allow | `SHARED-GROUP-REQ-020` |
| `bindings/mobile/src/mls/tests/group_management.rs` | `test_group_creation_custom_permissions` | Tokio multi-thread, 5 workers; mixed policy set and enforcement | `SHARED-GROUP-REQ-019` |
| `bindings/mobile/src/mls/tests/group_management.rs` | `test_group_creation_custom_permissions_fails_when_invalid` | Tokio multi-thread, 5 workers; three invalid and one valid option sets | `SHARED-GROUP-REQ-019` |
| `bindings/mobile/src/mls/tests/group_management.rs` | `test_update_policies_empty_group` | Tokio multi-thread, 5 workers; two-member and solo groups | `SHARED-GROUP-REQ-009` |
| `bindings/mobile/src/mls/tests/group_management.rs` | `test_can_stream_and_receive_metadata_update` | Tokio multi-thread, 5 workers; text, name update, and later text with active stream | `SHARED-GROUP-REQ-009`, `BIND-REQ-058` |
| `bindings/mobile/src/mls/tests/group_management.rs` | `test_disappearing_messages_deletion` | Tokio multi-thread, 5 workers; starts at latest message, 5 ns, then disable | `SHARED-GROUP-REQ-025` |
| `bindings/mobile/src/mls/tests/group_management.rs` | `test_disappearing_messages_with_0_from_ns_settings` | Tokio multi-thread, 5 workers; from_ns 0 and 5 ns | `SHARED-GROUP-REQ-025` |
| `bindings/mobile/src/mls/tests/group_management.rs` | `test_set_disappearing_messages_when_creating_group` | Tokio multi-thread, 5 workers; creation-time 2-second expiry | `SHARED-GROUP-REQ-025`, `SHARED-GROUP-REQ-008` |
| `bindings/mobile/src/mls/tests/group_management.rs` | `rapidfire_duplicate_create` | Tokio multi-thread, 10 workers; ten concurrent creates; ignored with d14n | `BIND-REQ-097` |
| `bindings/mobile/src/mls/tests/group_management.rs` | `test_group_who_added_me` | Tokio multi-thread, 1 worker; invitee welcome | `BIND-REQ-034` |
| `bindings/mobile/src/mls/tests/group_management.rs` | `test_conversation_debug_info_returns_correct_values` | Tokio multi-thread, 5 workers; fresh group | `BIND-REQ-034` |
| `bindings/mobile/src/mls/tests/group_management.rs` | `test_list_conversations_last_message` | Tokio multi-thread, 5 workers; two texts | `SHARED-GROUP-REQ-032` |
| `bindings/mobile/src/mls/tests/group_management.rs` | `test_list_conversations_no_messages` | Tokio multi-thread, 5 workers; only initial transcript | `SHARED-GROUP-REQ-032` |
| `bindings/mobile/src/mls/tests/group_management.rs` | `test_conversation_list_filters_readable_messages` | Tokio multi-thread, 5 workers; nine content-type cases | `SHARED-GROUP-REQ-032` |
| `bindings/mobile/src/mls/tests/group_management.rs` | `test_can_list_messages_with_content_types` | Tokio multi-thread, 5 workers; text and receipt, descending limit 1 | `BIND-REQ-049` |
| `bindings/mobile/src/mls/tests/group_management.rs` | `test_get_last_read_times` | Tokio async; one DM receipt | `SHARED-CONTENT-REQ-012` |
| `bindings/mobile/src/mls/tests/group_management.rs` | `test_pagination_of_conversations_list` | Tokio multi-thread, 5 workers; 15 groups and page size 5 | `SHARED-GROUP-REQ-037` |
| `bindings/mobile/src/mls/tests/group_management.rs` | `test_membership_state` | Tokio multi-thread, 1 worker; creator versus invitee | `BIND-REQ-034` |
| `bindings/mobile/src/mls/tests/content_types.rs` | `test_can_send_and_receive_reaction` | Tokio multi-thread, 5 workers; Unicode Added reaction and enriched query | `BIND-REQ-074` |
| `bindings/mobile/src/mls/tests/content_types.rs` | `test_reaction_encode_decode` | Tokio async; Added and Unicode | `BIND-REQ-074` |
| `bindings/mobile/src/mls/tests/content_types.rs` | `test_multi_remote_attachment_encode_decode` | Tokio async; two attachments | `BIND-REQ-078` |
| `bindings/mobile/src/mls/tests/content_types.rs` | `test_transaction_reference_roundtrip` | Tokio async; namespace and transfer metadata | `BIND-REQ-081` |
| `bindings/mobile/src/mls/tests/content_types.rs` | `test_attachment_roundtrip` | Tokio async; named text attachment | `BIND-REQ-077` |
| `bindings/mobile/src/mls/tests/content_types.rs` | `test_reply_roundtrip` | Tokio async; text body and reference inbox | `BIND-REQ-079` |
| `bindings/mobile/src/mls/tests/content_types.rs` | `test_read_receipt_roundtrip` | Tokio async; empty payload | `BIND-REQ-080` |
| `bindings/mobile/src/mls/tests/content_types.rs` | `test_remote_attachment_roundtrip` | Tokio async; all fields | `BIND-REQ-078` |
| `bindings/mobile/src/mls/tests/content_types.rs` | `test_long_messages` | Tokio multi-thread, 5 workers; about 100 KB DM payload | `BIND-REQ-046` |
| `bindings/mobile/src/mls/tests/content_types.rs` | `test_find_enriched_messages_with_reactions` | Tokio multi-thread, 1 worker; three texts and add, add, remove reactions | `BIND-REQ-074`, `BIND-REQ-088` |
| `bindings/mobile/src/mls/tests/content_types.rs` | `test_find_enriched_messages_with_replies` | Tokio multi-thread, 1 worker; DM texts and two replies | `BIND-REQ-079`, `BIND-REQ-088` |
| `bindings/mobile/src/mls/tests/content_types.rs` | `test_intent_codec` | Tokio async; complex, absent, oversized, malformed, and non-object metadata | `BIND-REQ-084` |
| `bindings/mobile/src/mls/tests/content_types.rs` | `test_actions_codec` | Tokio async; round trips and all validation cases | `BIND-REQ-083` |
| `bindings/mobile/src/mls/tests/content_types.rs` | `test_group_updated_codec` | Tokio async; basic, minimal, leave, null, complex, and invalid cases | `BIND-REQ-085` |
| `bindings/mobile/src/mls/tests/content_types.rs` | `test_leave_request_encode_decode` | Tokio async; no note, note, and invalid bytes | `BIND-REQ-086` |
| `bindings/mobile/src/mls/tests/content_types.rs` | `test_text_codec` | Tokio async; eight valid classes and invalid bytes | `BIND-REQ-075` |
| `bindings/mobile/src/mls/tests/content_types.rs` | `test_delete_message_encode_decode` | Tokio async; normal, empty, long, Unicode, and invalid | `BIND-REQ-087` |
| `bindings/mobile/src/mls/tests/streaming.rs` | `test_can_stream_group_messages_for_updates` | Tokio multi-thread, 5 workers; unconditionally ignored and wasm32-ignored; metadata, DM, and text sequence; intent checkpoints: Alix published/processed 2/2, Bo 1, Alix 3, Bo 3 then 4 | `BIND-REQ-055`, `BIND-REQ-058`, `BIND-REQ-023` |
| `bindings/mobile/src/mls/tests/streaming.rs` | `test_conversation_streaming` | Tokio multi-thread, 5 workers; two groups and close | `SHARED-GROUP-REQ-028` |
| `bindings/mobile/src/mls/tests/streaming.rs` | `test_stream_all_messages` | Tokio multi-thread, 5 workers; two groups and four interleaved messages | `BIND-REQ-055` |
| `bindings/mobile/src/mls/tests/streaming.rs` | `test_message_streaming` | Tokio multi-thread default workers; one group and two messages | `BIND-REQ-055` |
| `bindings/mobile/src/mls/tests/streaming.rs` | `test_message_streaming_when_removed_then_added` | Tokio multi-thread, 5 workers; before, during, and after membership change | `BIND-REQ-056` |
| `bindings/mobile/src/mls/tests/streaming.rs` | `test_stream_groups_gets_callback_when_streaming_messages` | Tokio multi-thread, 5 workers; overlapping group and message streams | `SHARED-GROUP-REQ-031` |
| `bindings/mobile/src/mls/tests/streaming.rs` | `test_stream_consent` | Tokio multi-thread, 5 workers; ignored with d14n; two installations | `BIND-REQ-100` |
| `bindings/mobile/src/mls/tests/streaming.rs` | `test_stream_preferences` | Tokio multi-thread, 5 workers; HMAC preference notification | `BIND-REQ-100` |
| `bindings/mobile/src/mls/tests/streaming.rs` | `test_overlapping_streams` | Tokio multi-thread, 5 workers; two conversation streams | `SHARED-GROUP-REQ-031` |
| `bindings/mobile/src/mls/tests/streaming.rs` | `test_can_stream_and_update_name_without_forking_group` | Tokio multi-thread, 5 workers; metadata and five callbacks; source title says without forking but no fork state is read | `BIND-REQ-058`, `SHARED-GROUP-REQ-009` |
| `bindings/mobile/src/mls/tests/streaming.rs` | `test_stream_all_messages_with_optimistic_group_creation` | Tokio multi-thread, 5 workers; two optimistic groups and three texts | `BIND-REQ-058` |
| `bindings/mobile/src/mls/tests/streaming.rs` | `test_stream_message_deletions_with_full_message_details` | Tokio multi-thread, 5 workers; one text deletion | `BIND-REQ-053` |
| `bindings/mobile/src/mls/tests/test_self_removal.rs` | `test_self_removal_with_pending_state` | Tokio multi-thread, 1 worker; bounded task-runner polling | `SHARED-GROUP-REQ-014` |
| `bindings/mobile/src/mls/tests/test_self_removal.rs` | `test_membership_state_after_readd` | Tokio multi-thread, 1 worker; leave, remove, and re-add | `SHARED-GROUP-REQ-015` |
| `bindings/mobile/src/mls/tests/test_self_removal.rs` | `test_creator_leave_and_readd_does_not_abort_welcome_stream` | Tokio multi-thread, 1 worker; cursor regression and stream startup | `BIND-REQ-037` |
| `bindings/mobile/src/mls/tests/test_self_removal.rs` | `test_leave_request_message_is_visible` | Tokio multi-thread, 1 worker; raw and enriched history | `BIND-REQ-038` |

### Node bindings

| File | Qualified test name | Form, gates, and cases | Requirements |
| --- | --- | --- | --- |
| `bindings/node/test/Builder.test.ts` | `BackendBuilder :: should build a Backend with default settings` | Vitest async; serial file | `BIND-REQ-002` |
| `bindings/node/test/Builder.test.ts` | `BackendBuilder :: should build with custom app version` | Vitest async | `BIND-REQ-002`, `BIND-REQ-006` |
| `bindings/node/test/Builder.test.ts` | `BackendBuilder :: should reject double build` | Vitest async; second build | `BIND-REQ-002` |
| `bindings/node/test/Builder.test.ts` | `NapiTestBuilder :: should set required fields and apply defaults` | Vitest sync; binding built with test-utils | `BIND-REQ-001` |
| `bindings/node/test/Builder.test.ts` | `NapiTestBuilder :: should support setter chaining` | Vitest sync; full chain | `BIND-REQ-001` |
| `bindings/node/test/Builder.test.ts` | `NapiTestBuilder :: should support partial chaining` | Vitest sync; one setter | `BIND-REQ-001` |
| `bindings/node/test/Builder.test.ts` | `NapiTestBuilder :: should allow defaults to be overridden` | Vitest sync | `BIND-REQ-001` |
| `bindings/node/test/CatchUp.test.ts` | `catchUpToLive :: cold-catches a pending group and its history, then is idempotent` | Vitest async; local backend; two runs | `BIND-REQ-061` |
| `bindings/node/test/Client.test.ts` | `Client :: should not be registered at first` | Vitest async | `SHARED-IDENTITY-REQ-001`, `BIND-REQ-006` |
| `bindings/node/test/Client.test.ts` | `Client :: should return client versions` | Vitest async; custom app version | `BIND-REQ-006` |
| `bindings/node/test/Client.test.ts` | `Client :: should be registered after registration` | Vitest async; fresh client after prior registration | `SHARED-IDENTITY-REQ-001` |
| `bindings/node/test/Client.test.ts` | `Client :: should be able to message registered identity` | Vitest async | `SHARED-IDENTITY-REQ-001` |
| `bindings/node/test/Client.test.ts` | `Client :: should find an inbox ID from an address` | Vitest async | `SHARED-IDENTITY-REQ-001` |
| `bindings/node/test/Client.test.ts` | `Client :: should return the correct inbox state` | Vitest async; refresh false and true | `SHARED-IDENTITY-REQ-002` |
| `bindings/node/test/Client.test.ts` | `Client :: should add a wallet association to the client` | Vitest async; second ECDSA wallet | `SHARED-IDENTITY-REQ-003` |
| `bindings/node/test/Client.test.ts` | `Client :: should revoke a wallet association from the client` | Vitest async; add then revoke | `SHARED-IDENTITY-REQ-003` |
| `bindings/node/test/Client.test.ts` | `Client :: should revoke all installations` | Vitest async; three installations | `SHARED-IDENTITY-REQ-006` |
| `bindings/node/test/Client.test.ts` | `Client :: should revoke a specific installation using static_revoke_installations` | Vitest async; five installations | `SHARED-IDENTITY-REQ-006` |
| `bindings/node/test/Client.test.ts` | `Client :: should manage consent states` | Vitest async; Unknown to Allowed to Denied | `SHARED-GROUP-REQ-021` |
| `bindings/node/test/Client.test.ts` | `Client :: should get inbox addresses` | Vitest async; two independent inboxes | `SHARED-IDENTITY-REQ-002` |
| `bindings/node/test/Client.test.ts` | `Client :: should get inbox state statically` | Vitest async; two installations | `SHARED-IDENTITY-REQ-002` |
| `bindings/node/test/Client.test.ts` | `Client :: should sign and verify with installation key` | Vitest async; instance and static verification plus empty bytes | `SHARED-IDENTITY-REQ-005` |
| `bindings/node/test/Client.test.ts` | `Client :: should release and reconnect database connection` | Vitest async | `SHARED-IDENTITY-REQ-019` |
| `bindings/node/test/Client.test.ts` | `Client :: should close cleanly and be idempotent` | Vitest async; two closes and refused reconnect | `SHARED-IDENTITY-REQ-019` |
| `bindings/node/test/Client.test.ts` | `Streams :: should stream all messages` | Vitest async; four texts | `BIND-REQ-055` |
| `bindings/node/test/Conversation.test.ts` | `Conversation :: should update conversation name` | Vitest async in describe.concurrent; local and remote | `SHARED-GROUP-REQ-009` |
| `bindings/node/test/Conversation.test.ts` | `Conversation :: should update conversation image URL` | Vitest async in describe.concurrent; local and remote | `SHARED-GROUP-REQ-009` |
| `bindings/node/test/Conversation.test.ts` | `Conversation :: should update conversation description` | Vitest async in describe.concurrent; local and remote | `SHARED-GROUP-REQ-009` |
| `bindings/node/test/Conversation.test.ts` | `Conversation :: should add and remove members` | Vitest async in describe.concurrent; identity API; valid add-result fields checked, remove-result fields not checked | `SHARED-GROUP-REQ-011` |
| `bindings/node/test/Conversation.test.ts` | `Conversation :: should add and remove members by inbox id` | Vitest async in describe.concurrent; inbox API; valid add-result fields checked, remove-result fields not checked | `SHARED-GROUP-REQ-011` |
| `bindings/node/test/Conversation.test.ts` | `Conversation :: should send and list messages` | Vitest async in describe.concurrent; sender and recipient | `SHARED-GROUP-REQ-026` |
| `bindings/node/test/Conversation.test.ts` | `Conversation :: should optimistically send and list messages` | Vitest async in describe.concurrent; before and after publish | `SHARED-CONTENT-REQ-002` |
| `bindings/node/test/Conversation.test.ts` | `Conversation :: should produce deterministic ids for a caller-set idempotency key` | Vitest async in describe.concurrent; repeated, different, and absent key | `SHARED-CONTENT-REQ-003` |
| `bindings/node/test/Conversation.test.ts` | `Conversation :: should stream messages` | Vitest async in describe.concurrent; two messages | `BIND-REQ-055` |
| `bindings/node/test/Conversation.test.ts` | `Conversation :: should add and remove admins` | Vitest async in describe.concurrent | `SHARED-GROUP-REQ-017` |
| `bindings/node/test/Conversation.test.ts` | `Conversation :: should add and remove super admins` | Vitest async in describe.concurrent | `SHARED-GROUP-REQ-017` |
| `bindings/node/test/Conversation.test.ts` | `Conversation :: should manage group consent state` | Vitest async in describe.concurrent; group and DM; send promotes consent | `SHARED-GROUP-REQ-021` |
| `bindings/node/test/Conversation.test.ts` | `Conversation :: should update group permissions` | Vitest async in describe.concurrent; seven update operations | `SHARED-GROUP-REQ-020` |
| `bindings/node/test/Conversation.test.ts` | `Conversation :: should get hmac keys` | Vitest async in describe.concurrent; one group; three 42-byte keys with bigint epochs | `SHARED-GROUP-REQ-041` |
| `bindings/node/test/Conversation.test.ts` | `Conversation :: should get membership state` | Vitest async in describe.concurrent; creator and invitee | `BIND-REQ-034` |
| `bindings/node/test/Conversations.test.ts` | `Conversations :: should not have initial conversations` | Vitest async; serial suite | `SHARED-GROUP-REQ-007` |
| `bindings/node/test/Conversations.test.ts` | `Conversations :: should create a group chat` | Vitest async; full default property and list contract | `SHARED-GROUP-REQ-007`, `SHARED-GROUP-REQ-018` |
| `bindings/node/test/Conversations.test.ts` | `Conversations :: should create a group with custom permissions` | Vitest async; one full custom set | `SHARED-GROUP-REQ-019` |
| `bindings/node/test/Conversations.test.ts` | `Conversations :: should update group permission policy` | Vitest async; AddAdmin and GroupName | `SHARED-GROUP-REQ-020` |
| `bindings/node/test/Conversations.test.ts` | `Conversations :: should create a dm group` | Vitest async; full DM property, list, and peer contract | `SHARED-GROUP-REQ-001`, `SHARED-GROUP-REQ-018`, `SHARED-GROUP-REQ-006`, `BIND-REQ-098` |
| `bindings/node/test/Conversations.test.ts` | `Conversations :: should find a group by ID` | Vitest async | `BIND-REQ-098` |
| `bindings/node/test/Conversations.test.ts` | `Conversations :: should find a message by ID` | Vitest async | `BIND-REQ-098` |
| `bindings/node/test/Conversations.test.ts` | `Conversations :: should create a new group with options` | Vitest async; name, image, both, admin-only, and description | `SHARED-GROUP-REQ-008`, `SHARED-GROUP-REQ-018` |
| `bindings/node/test/Conversations.test.ts` | `Conversations :: should update group metadata` | Vitest async; name, image, and description | `SHARED-GROUP-REQ-009` |
| `bindings/node/test/Conversations.test.ts` | `Conversations :: should stream all groups` | Vitest async; two groups and one DM; order | `SHARED-GROUP-REQ-028` |
| `bindings/node/test/Conversations.test.ts` | `Conversations :: should error when connection dies` | Vitest async; 45-second timeout; toxic proxy; keepalive 10/10 seconds | `BIND-REQ-059` |
| `bindings/node/test/Conversations.test.ts` | `Conversations :: should only stream group chats` | Vitest async; Group filter | `SHARED-GROUP-REQ-028` |
| `bindings/node/test/Conversations.test.ts` | `Conversations :: should only stream dm groups` | Vitest async; DM filter | `SHARED-GROUP-REQ-028` |
| `bindings/node/test/Conversations.test.ts` | `Conversations :: should stream all messages` | Vitest async; three conversation types and four participants | `BIND-REQ-055` |
| `bindings/node/test/Conversations.test.ts` | `Conversations :: should only stream group chat messages` | Vitest async; Group filter | `BIND-REQ-055` |
| `bindings/node/test/Conversations.test.ts` | `Conversations :: should only stream dm messages` | Vitest async; DM filter | `BIND-REQ-055` |
| `bindings/node/test/Conversations.test.ts` | `Conversations :: stream should process dm messages from new installations without sync` | Vitest async; second installation | `BIND-REQ-055`, `BIND-REQ-042` |
| `bindings/node/test/Conversations.test.ts` | `Conversations :: should get hmac keys` | Vitest async; collection includes one group and one DM; each has three 42-byte keys with bigint epochs | `SHARED-GROUP-REQ-041` |
| `bindings/node/test/Conversations.test.ts` | `Conversations :: should sync groups across installations` | Vitest async; second installation, group and DM | `BIND-REQ-042` |
| `bindings/node/test/Conversations.test.ts` | `Conversations :: should create initial group updated messages for added members` | Vitest async; removal, re-add, and second installation | `BIND-REQ-085`, `BIND-REQ-042` |
| `bindings/node/test/Conversations.test.ts` | `Conversations :: should stream deleted messages` | Vitest async; one deletion | `BIND-REQ-053` |
| `bindings/node/test/EnrichedMessage.test.ts` | `EnrichedMessage > Basic message retrieval :: should return enriched messages with basic fields populated` | Vitest async in describe.concurrent; two texts and initial update | `BIND-REQ-088` |
| `bindings/node/test/EnrichedMessage.test.ts` | `EnrichedMessage > Basic message retrieval :: should handle list options` | Vitest async in describe.concurrent; descending limit 2 | `BIND-REQ-088` |
| `bindings/node/test/EnrichedMessage.test.ts` | `EnrichedMessage > Message metadata :: should include message kind` | Vitest async in describe.concurrent | `BIND-REQ-088` |
| `bindings/node/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Text :: should send and receive text message` | Vitest async in describe.concurrent | `BIND-REQ-075` |
| `bindings/node/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Markdown :: should send and receive a markdown message` | Vitest async in describe.concurrent | `BIND-REQ-076` |
| `bindings/node/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Reaction :: should send and receive reaction with Added action` | Vitest async in describe.concurrent; Unicode, fallback, and parent | `BIND-REQ-074` |
| `bindings/node/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Reaction :: should send and receive reaction with Removed action` | Vitest async in describe.concurrent; add then remove | `BIND-REQ-074` |
| `bindings/node/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Reaction :: should handle shortcode reaction schema` | Vitest async in describe.concurrent | `BIND-REQ-074` |
| `bindings/node/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Reaction :: should handle custom reaction schema` | Vitest async in describe.concurrent | `BIND-REQ-074` |
| `bindings/node/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Reply :: should send and receive reply with text content` | Vitest async in describe.concurrent | `BIND-REQ-079` |
| `bindings/node/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Reply :: should include inReplyTo with original message` | Vitest async in describe.concurrent | `BIND-REQ-079` |
| `bindings/node/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Reply :: should send and receive reply with non-text content (attachment)` | Vitest async in describe.concurrent | `BIND-REQ-079`, `BIND-REQ-077` |
| `bindings/node/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Attachment :: should send and receive attachment` | Vitest async in describe.concurrent; filename | `BIND-REQ-077` |
| `bindings/node/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Attachment :: should send and receive attachment without filename` | Vitest async in describe.concurrent | `BIND-REQ-077` |
| `bindings/node/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Remote Attachment :: should send and receive remote attachment` | Vitest async in describe.concurrent; all fields and fallback | `BIND-REQ-078` |
| `bindings/node/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Remote Attachment :: should send and receive remote attachment without filename` | Vitest async in describe.concurrent | `BIND-REQ-078` |
| `bindings/node/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Multi Remote Attachment :: should send and receive multi remote attachment` | Vitest async in describe.concurrent; two entries | `BIND-REQ-078` |
| `bindings/node/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Multi Remote Attachment :: should send and receive multi remote attachment with single attachment` | Vitest async in describe.concurrent | `BIND-REQ-078` |
| `bindings/node/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Read Receipt :: should send read receipt (excluded from enriched messages by design)` | Vitest async in describe.concurrent | `BIND-REQ-080` |
| `bindings/node/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Transaction Reference :: should send and receive transaction reference` | Vitest async in describe.concurrent; namespace, reference, and fallback | `BIND-REQ-081` |
| `bindings/node/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Transaction Reference :: should send and receive transaction reference without namespace` | Vitest async in describe.concurrent | `BIND-REQ-081` |
| `bindings/node/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Transaction Reference :: should send and receive transaction reference with empty reference` | Vitest async in describe.concurrent | `BIND-REQ-081` |
| `bindings/node/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Transaction Reference :: should send and receive transaction reference with metadata` | Vitest async in describe.concurrent | `BIND-REQ-081` |
| `bindings/node/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Wallet Send Calls :: should send and receive wallet send calls` | Vitest async in describe.concurrent; one call | `SHARED-CONTENT-REQ-014` |
| `bindings/node/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Wallet Send Calls :: should send and receive wallet send calls with multiple calls` | Vitest async in describe.concurrent; two calls and gas | `SHARED-CONTENT-REQ-014` |
| `bindings/node/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Wallet Send Calls :: should send and receive wallet send calls with metadata` | Vitest async in describe.concurrent; note and paymaster | `SHARED-CONTENT-REQ-014` |
| `bindings/node/test/EnrichedMessage.test.ts` | ``EnrichedMessage > Content types > Wallet Send Calls :: should error when metadata is missing `description` field`` | Vitest async in describe.concurrent; negative | `SHARED-CONTENT-REQ-014` |
| `bindings/node/test/EnrichedMessage.test.ts` | ``EnrichedMessage > Content types > Wallet Send Calls :: should error when metadata is missing `transactionType` field`` | Vitest async in describe.concurrent; negative | `SHARED-CONTENT-REQ-014` |
| `bindings/node/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Actions :: should send and receive actions` | Vitest async in describe.concurrent; two actions and fallback | `BIND-REQ-083` |
| `bindings/node/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Actions :: should send and receive actions with all styles` | Vitest async in describe.concurrent; Primary, Secondary, and Danger | `BIND-REQ-083` |
| `bindings/node/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Actions :: should send and receive actions with expiration` | Vitest async in describe.concurrent; set and item timestamps | `BIND-REQ-083` |
| `bindings/node/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Actions :: should send and receive actions with image URL` | Vitest async in describe.concurrent | `BIND-REQ-083` |
| `bindings/node/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Intent :: should send and receive intent` | Vitest async in describe.concurrent | `BIND-REQ-084` |
| `bindings/node/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Intent :: should send and receive intent with metadata` | Vitest async in describe.concurrent | `BIND-REQ-084` |
| `bindings/node/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Group Updated :: should include group updated messages when members are added` | Vitest async in describe.concurrent | `BIND-REQ-085` |
| `bindings/node/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Group Updated :: should include group updated messages when members are removed` | Vitest async in describe.concurrent | `BIND-REQ-085` |
| `bindings/node/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Group Updated :: should include group updated messages when metadata is changed` | Vitest async in describe.concurrent | `BIND-REQ-085` |
| `bindings/node/test/RemoteAttachmentEncryption.test.ts` | `RemoteAttachment encryption compatibility :: should decrypt TS encrypted payload with Rust` | Vitest async; TypeScript to Rust | `BIND-REQ-090` |
| `bindings/node/test/RemoteAttachmentEncryption.test.ts` | `RemoteAttachment encryption compatibility :: should decrypt Rust encrypted payload with TS` | Vitest async; Rust to TypeScript; mocked fetch | `BIND-REQ-090` |
| `bindings/node/test/RemoteAttachmentEncryption.test.ts` | `RemoteAttachment encryption compatibility :: should fail with wrong content digest` | Vitest sync | `BIND-REQ-090` |
| `bindings/node/test/RemoteAttachmentEncryption.test.ts` | `RemoteAttachment encryption compatibility :: should fail with wrong secret` | Vitest sync; random 32-byte key | `BIND-REQ-090` |
| `bindings/node/test/RemoteAttachmentEncryption.test.ts` | `RemoteAttachment encryption compatibility :: should fail with corrupted payload` | Vitest sync; first byte flipped | `BIND-REQ-090` |
| `bindings/node/test/RemoteAttachmentEncryption.test.ts` | `RemoteAttachment encryption compatibility :: should create a 32-byte secret` | Vitest sync | `BIND-REQ-090` |
| `bindings/node/test/RemoteAttachmentEncryption.test.ts` | `RemoteAttachment encryption compatibility :: should create a 32-byte salt` | Vitest sync | `BIND-REQ-090` |
| `bindings/node/test/RemoteAttachmentEncryption.test.ts` | `RemoteAttachment encryption compatibility :: should create a 12-byte nonce` | Vitest sync | `BIND-REQ-090` |
| `bindings/node/test/RemoteAttachmentEncryption.test.ts` | `RemoteAttachment encryption compatibility :: should produce unique encryption each time` | Vitest sync; two runs | `BIND-REQ-090` |
| `bindings/node/test/inboxId.test.ts` | `generateInboxId :: should generate an inbox id` | Vitest sync; valid Ethereum address | `SHARED-IDENTITY-REQ-001` |
| `bindings/node/test/inboxId.test.ts` | `generateInboxId :: should throw error with [ErrorType::Variant] format for invalid address` | Vitest sync; invalid address | `BIND-REQ-005`, `SHARED-IDENTITY-REQ-001` |
| `bindings/node/test/inboxId.test.ts` | `getInboxIdByIdentity :: should return`null`inbox ID for unregistered address` | Vitest async | `SHARED-IDENTITY-REQ-001` |
| `bindings/node/test/inboxId.test.ts` | `getInboxIdByIdentity :: should return inbox ID for registered address` | Vitest async | `SHARED-IDENTITY-REQ-001` |
| `bindings/node/test/inboxId.test.ts` | `isInstallationAuthorized :: should return true if installation is authorized` | Vitest async | `SHARED-IDENTITY-REQ-002` |
| `bindings/node/test/inboxId.test.ts` | `isAddressAuthorized :: should return true if address is authorized` | Vitest async | `SHARED-IDENTITY-REQ-002` |
| `bindings/node/test/initLogging.test.ts` | `initLogging :: does not panic without an OTLP endpoint` | Vitest async; process-global first call | `BIND-REQ-007` |
| `bindings/node/test/initLogging.test.ts` | `initLogging :: does not panic when an OTLP endpoint is configured (the boot-panic regression)` | Vitest async; non-listening endpoint | `BIND-REQ-007` |
| `bindings/node/test/initLogging.test.ts` | `initLogging :: is idempotent — a second call is a no-op and still does not throw` | Vitest async; prior same-file initialization | `BIND-REQ-007` |

### WebAssembly bindings

| File | Qualified test name | Form, gates, and cases | Requirements |
| --- | --- | --- | --- |
| `bindings/wasm/src/conversation.rs` | `conversation::tests::test_group_message_to_object` | wasm_bindgen_test; cfg(test) in wasm-only crate; dedicated worker | `BIND-REQ-094` |
| `bindings/wasm/test/Builder.test.ts` | `WasmTestBuilder :: should set required fields and apply defaults` | Vitest sync; Chromium and Firefox; test-utils build | `BIND-REQ-001` |
| `bindings/wasm/test/Builder.test.ts` | `WasmTestBuilder :: should support setter chaining` | Vitest sync; Chromium and Firefox; full chain | `BIND-REQ-001` |
| `bindings/wasm/test/Builder.test.ts` | `WasmTestBuilder :: should support partial chaining` | Vitest sync; Chromium and Firefox | `BIND-REQ-001` |
| `bindings/wasm/test/Builder.test.ts` | `WasmTestBuilder :: should allow defaults to be overridden` | Vitest sync; Chromium and Firefox | `BIND-REQ-001` |
| `bindings/wasm/test/client.test.ts` | `streams groups local` | Vitest async; Chromium and Firefox; ephemeral database; three-group ReadableStream | `SHARED-GROUP-REQ-028` |
| `bindings/wasm/test/client.test.ts` | `streams groups` | Vitest async; Chromium and Firefox; callback stream | `SHARED-GROUP-REQ-028` |
| `bindings/wasm/test/client.test.ts` | `auth callback` | Vitest async; Chromium and Firefox; bearer token and expiry | `BIND-REQ-095` |
| `bindings/wasm/test/client.test.ts` | `auth callback throws error` | Vitest async; Chromium and Firefox; thrown JavaScript Error | `BIND-REQ-095` |
| `bindings/wasm/test/Conversations.test.ts` | `Conversations :: should stream deleted messages` | Vitest async; Chromium and Firefox; one deletion | `BIND-REQ-053` |
| `bindings/wasm/test/Conversations.test.ts` | `Conversations :: should produce deterministic ids for a caller-set idempotency key` | Vitest async; Chromium and Firefox; repeated, different, and absent key | `SHARED-CONTENT-REQ-003` |
| `bindings/wasm/test/errorCodes.test.ts` | `Error Codes :: should include error code in message when updating group name exceeds character limit` | Vitest async; Chromium and Firefox; 1,025 characters | `BIND-REQ-005` |
| `bindings/wasm/test/errorCodes.test.ts` | `Error Codes :: should include error code in message when adding invalid member` | Vitest async; Chromium and Firefox; invalid inbox ID | `BIND-REQ-005` |
| `bindings/wasm/test/RemoteAttachmentEncryption.test.ts` | `RemoteAttachment encryption compatibility :: should decrypt TS encrypted payload with WASM` | Vitest async; Chromium and Firefox; TypeScript to Wasm | `BIND-REQ-090` |
| `bindings/wasm/test/RemoteAttachmentEncryption.test.ts` | `RemoteAttachment encryption compatibility :: should decrypt WASM encrypted payload with TS` | Vitest async; Chromium and Firefox; Wasm to TypeScript; mocked fetch | `BIND-REQ-090` |
| `bindings/wasm/test/RemoteAttachmentEncryption.test.ts` | `RemoteAttachment encryption compatibility :: should fail with wrong content digest` | Vitest sync; Chromium and Firefox | `BIND-REQ-090` |
| `bindings/wasm/test/RemoteAttachmentEncryption.test.ts` | `RemoteAttachment encryption compatibility :: should fail with wrong secret` | Vitest sync; Chromium and Firefox; random 32-byte key | `BIND-REQ-090` |
| `bindings/wasm/test/RemoteAttachmentEncryption.test.ts` | `RemoteAttachment encryption compatibility :: should fail with corrupted payload` | Vitest sync; Chromium and Firefox; first byte flipped | `BIND-REQ-090` |
| `bindings/wasm/test/RemoteAttachmentEncryption.test.ts` | `RemoteAttachment encryption compatibility :: should create a 32-byte secret` | Vitest sync; Chromium and Firefox | `BIND-REQ-090` |
| `bindings/wasm/test/RemoteAttachmentEncryption.test.ts` | `RemoteAttachment encryption compatibility :: should create a 32-byte salt` | Vitest sync; Chromium and Firefox | `BIND-REQ-090` |
| `bindings/wasm/test/RemoteAttachmentEncryption.test.ts` | `RemoteAttachment encryption compatibility :: should create a 12-byte nonce` | Vitest sync; Chromium and Firefox | `BIND-REQ-090` |
| `bindings/wasm/test/RemoteAttachmentEncryption.test.ts` | `RemoteAttachment encryption compatibility :: should produce unique encryption each time` | Vitest sync; Chromium and Firefox; two runs | `BIND-REQ-090` |
| `bindings/wasm/test/opfs.test.ts` | `OPFS File Management > Basic OPFS operations :: should list files (initially empty after clear)` | Vitest async; Chromium and Firefox; dedicated Worker; clear before each | `BIND-REQ-091` |
| `bindings/wasm/test/opfs.test.ts` | `OPFS File Management > Basic OPFS operations :: should report file count as 0 after clear` | Vitest async; Chromium and Firefox; Worker | `BIND-REQ-091` |
| `bindings/wasm/test/opfs.test.ts` | `OPFS File Management > Basic OPFS operations :: should report pool capacity` | Vitest async; Chromium and Firefox; capacity at least 6 | `BIND-REQ-091` |
| `bindings/wasm/test/opfs.test.ts` | `OPFS File Management > Basic OPFS operations :: should return false for non-existent file` | Vitest async; Chromium and Firefox | `BIND-REQ-091` |
| `bindings/wasm/test/opfs.test.ts` | `OPFS File Management > Basic OPFS operations :: should return false when deleting non-existent file` | Vitest async; Chromium and Firefox | `BIND-REQ-091` |
| `bindings/wasm/test/opfs.test.ts` | `OPFS File Management > OPFS with persistent client :: should create database file when client is created` | Vitest async; Chromium and Firefox; persistent client | `BIND-REQ-092` |
| `bindings/wasm/test/opfs.test.ts` | `OPFS File Management > OPFS with persistent client :: should create multiple database files` | Vitest async; Chromium and Firefox; two names | `BIND-REQ-092` |
| `bindings/wasm/test/opfs.test.ts` | `OPFS File Management > OPFS with persistent client :: should delete a specific database file` | Vitest async; Chromium and Firefox | `BIND-REQ-092` |
| `bindings/wasm/test/opfs.test.ts` | `OPFS File Management > OPFS with persistent client :: should clear all database files` | Vitest async; Chromium and Firefox; two files | `BIND-REQ-092` |
| `bindings/wasm/test/opfs.test.ts` | `OPFS File Management > Database export and import :: should export a database file` | Vitest async; Chromium and Firefox; SQLite header | `BIND-REQ-093` |
| `bindings/wasm/test/opfs.test.ts` | `OPFS File Management > Database export and import :: should import a database file` | Vitest async; Chromium and Firefox; renamed copy | `BIND-REQ-093` |
| `bindings/wasm/test/opfs.test.ts` | `OPFS File Management > Database export and import :: should replace database by deleting then importing` | Vitest async; Chromium and Firefox; restore original bytes and size | `BIND-REQ-093` |
| `bindings/wasm/test/opfs.test.ts` | `OPFS File Management > Database export and import :: should fail to export non-existent database` | Vitest async; Chromium and Firefox; negative | `BIND-REQ-093` |
| `bindings/wasm/test/opfs.test.ts` | `OPFS File Management > Database export and import :: should fail to import invalid data` | Vitest async; Chromium and Firefox; bytes 1 through 5 | `BIND-REQ-093` |
| `bindings/wasm/test/opfs.test.ts` | `OPFS File Management > Database export and import :: should roundtrip export and import` | Vitest async; Chromium and Firefox; byte equality | `BIND-REQ-093` |
| `bindings/wasm/test/EnrichedMessage.test.ts` | `EnrichedMessage > Basic message retrieval :: should return enriched messages with basic fields populated` | Vitest async; Chromium and Firefox; two texts and initial update | `BIND-REQ-088` |
| `bindings/wasm/test/EnrichedMessage.test.ts` | `EnrichedMessage > Basic message retrieval :: should handle list options` | Vitest async; Chromium and Firefox; descending limit 2 | `BIND-REQ-088` |
| `bindings/wasm/test/EnrichedMessage.test.ts` | `EnrichedMessage > Message metadata :: should include message kind` | Vitest async; Chromium and Firefox | `BIND-REQ-088` |
| `bindings/wasm/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Text :: should send and receive text message` | Vitest async; Chromium and Firefox | `BIND-REQ-075` |
| `bindings/wasm/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Text :: should have correct content type` | Vitest sync descriptor; Chromium and Firefox | `BIND-REQ-089` |
| `bindings/wasm/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Markdown :: should send and receive markdown messages` | Vitest async; Chromium and Firefox | `BIND-REQ-076` |
| `bindings/wasm/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Markdown :: should have correct content type` | Vitest sync descriptor; Chromium and Firefox | `BIND-REQ-089` |
| `bindings/wasm/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Reaction :: should send and receive reaction with Added action` | Vitest async; Chromium and Firefox; Unicode, fallback, and parent | `BIND-REQ-074` |
| `bindings/wasm/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Reaction :: should send and receive reaction with Removed action` | Vitest async; Chromium and Firefox; add then remove | `BIND-REQ-074` |
| `bindings/wasm/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Reaction :: should handle shortcode reaction schema` | Vitest async; Chromium and Firefox | `BIND-REQ-074` |
| `bindings/wasm/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Reaction :: should handle custom reaction schema` | Vitest async; Chromium and Firefox | `BIND-REQ-074` |
| `bindings/wasm/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Reaction :: should have correct content type` | Vitest sync descriptor; Chromium and Firefox | `BIND-REQ-089` |
| `bindings/wasm/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Reply :: should send and receive reply with text content` | Vitest async; Chromium and Firefox | `BIND-REQ-079` |
| `bindings/wasm/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Reply :: should include inReplyTo with original message` | Vitest async; Chromium and Firefox | `BIND-REQ-079` |
| `bindings/wasm/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Reply :: should send and receive reply with non-text content (attachment)` | Vitest async; Chromium and Firefox | `BIND-REQ-079`, `BIND-REQ-077` |
| `bindings/wasm/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Reply :: should have correct content type` | Vitest sync descriptor; Chromium and Firefox | `BIND-REQ-089` |
| `bindings/wasm/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Attachment :: should send and receive attachment` | Vitest async; Chromium and Firefox; filename | `BIND-REQ-077` |
| `bindings/wasm/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Attachment :: should send and receive attachment without filename` | Vitest async; Chromium and Firefox | `BIND-REQ-077` |
| `bindings/wasm/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Attachment :: should have correct content type` | Vitest sync descriptor; Chromium and Firefox | `BIND-REQ-089` |
| `bindings/wasm/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Remote Attachment :: should send and receive remote attachment` | Vitest async; Chromium and Firefox; all fields and fallback | `BIND-REQ-078` |
| `bindings/wasm/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Remote Attachment :: should send and receive remote attachment without filename` | Vitest async; Chromium and Firefox | `BIND-REQ-078` |
| `bindings/wasm/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Remote Attachment :: should have correct content type` | Vitest sync descriptor; Chromium and Firefox | `BIND-REQ-089` |
| `bindings/wasm/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Multi Remote Attachment :: should send and receive multi remote attachment` | Vitest async; Chromium and Firefox; two entries | `BIND-REQ-078` |
| `bindings/wasm/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Multi Remote Attachment :: should send and receive multi remote attachment with single attachment` | Vitest async; Chromium and Firefox | `BIND-REQ-078` |
| `bindings/wasm/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Multi Remote Attachment :: should have correct content type` | Vitest sync descriptor; Chromium and Firefox | `BIND-REQ-089` |
| `bindings/wasm/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Read Receipt :: should send read receipt (excluded from enriched messages by design)` | Vitest async; Chromium and Firefox | `BIND-REQ-080` |
| `bindings/wasm/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Read Receipt :: should have correct content type` | Vitest sync descriptor; Chromium and Firefox | `BIND-REQ-089` |
| `bindings/wasm/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Transaction Reference :: should send and receive transaction reference` | Vitest async; Chromium and Firefox; namespace, reference, and fallback | `BIND-REQ-081` |
| `bindings/wasm/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Transaction Reference :: should send and receive transaction reference without namespace` | Vitest async; Chromium and Firefox | `BIND-REQ-081` |
| `bindings/wasm/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Transaction Reference :: should send and receive transaction reference with empty reference` | Vitest async; Chromium and Firefox | `BIND-REQ-081` |
| `bindings/wasm/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Transaction Reference :: should send and receive transaction reference with metadata` | Vitest async; Chromium and Firefox | `BIND-REQ-081` |
| `bindings/wasm/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Transaction Reference :: should have correct content type` | Vitest sync descriptor; Chromium and Firefox | `BIND-REQ-089` |
| `bindings/wasm/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Wallet Send Calls :: should send and receive wallet send calls` | Vitest async; Chromium and Firefox; one call | `SHARED-CONTENT-REQ-014` |
| `bindings/wasm/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Wallet Send Calls :: should send and receive wallet send calls with multiple calls` | Vitest async; Chromium and Firefox; two calls and gas | `SHARED-CONTENT-REQ-014` |
| `bindings/wasm/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Wallet Send Calls :: should send and receive wallet send calls with metadata` | Vitest async; Chromium and Firefox; note and paymaster | `SHARED-CONTENT-REQ-014` |
| `bindings/wasm/test/EnrichedMessage.test.ts` | ``EnrichedMessage > Content types > Wallet Send Calls :: should error when metadata is missing `description` field`` | Vitest async; Chromium and Firefox; exact binding error | `SHARED-CONTENT-REQ-014`, `BIND-REQ-005` |
| `bindings/wasm/test/EnrichedMessage.test.ts` | ``EnrichedMessage > Content types > Wallet Send Calls :: should error when metadata is missing `transactionType` field`` | Vitest async; Chromium and Firefox; exact binding error | `SHARED-CONTENT-REQ-014`, `BIND-REQ-005` |
| `bindings/wasm/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Wallet Send Calls :: should have correct content type` | Vitest sync descriptor; Chromium and Firefox | `BIND-REQ-089` |
| `bindings/wasm/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Actions :: should send and receive actions` | Vitest async; Chromium and Firefox; two actions and fallback | `BIND-REQ-083` |
| `bindings/wasm/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Actions :: should send and receive actions with all styles` | Vitest async; Chromium and Firefox; Primary, Secondary, and Danger | `BIND-REQ-083` |
| `bindings/wasm/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Actions :: should send and receive actions with expiration` | Vitest async; Chromium and Firefox; set and item timestamps | `BIND-REQ-083` |
| `bindings/wasm/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Actions :: should send and receive actions with image URL` | Vitest async; Chromium and Firefox | `BIND-REQ-083` |
| `bindings/wasm/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Actions :: should have correct content type` | Vitest sync descriptor; Chromium and Firefox | `BIND-REQ-089` |
| `bindings/wasm/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Intent :: should send and receive intent using encodeIntent` | Vitest async; Chromium and Firefox; generic send path | `BIND-REQ-084` |
| `bindings/wasm/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Intent :: should send and receive intent using sendIntent` | Vitest async; Chromium and Firefox; convenience path | `BIND-REQ-084` |
| `bindings/wasm/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Intent :: should send and receive intent with metadata` | Vitest async; Chromium and Firefox | `BIND-REQ-084` |
| `bindings/wasm/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Intent :: should have correct content type` | Vitest sync descriptor; Chromium and Firefox | `BIND-REQ-089` |
| `bindings/wasm/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Group Updated :: should include group updated messages when members are added` | Vitest async; Chromium and Firefox | `BIND-REQ-085` |
| `bindings/wasm/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Group Updated :: should include group updated messages when members are removed` | Vitest async; Chromium and Firefox | `BIND-REQ-085` |
| `bindings/wasm/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Group Updated :: should include group updated messages when metadata is changed` | Vitest async; Chromium and Firefox | `BIND-REQ-085` |
| `bindings/wasm/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Group Updated :: should have correct content type` | Vitest sync descriptor; Chromium and Firefox | `BIND-REQ-089` |
| `bindings/wasm/test/EnrichedMessage.test.ts` | `EnrichedMessage > Content types > Leave Request :: should have correct content type` | Vitest sync descriptor; Chromium and Firefox | `BIND-REQ-089` |

Runner notes: the Node suite builds the N-API binding with `test-utils`, uses Node 22 or later, local xmtpd, and a 30-second default timeout. The Wasm Vitest suite builds with `test-utils` and runs every declaration in headless Chromium and Firefox with a 60-second timeout. Wasm OPFS cases use a dedicated Worker. The Rust Wasm case uses a dedicated wasm-bindgen worker. No executable doctests or parameterized or property declarations exist. The Android example tests are source declarations but have no Gradle project under `bindings/mobile`.
