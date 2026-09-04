# xmtp_mls group test index

[← Test inventory](../existing-tests.md) · [Requirements](../existing-requirements.md)

| File | Qualified test | Form / gates / cases | Requirements |
| --- | --- | --- | --- |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_send_message | — | `SHARED-CONTENT-REQ-001` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_receive_self_message | — | `SHARED-GROUP-REQ-026` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_receive_message_from_other | — | `SHARED-GROUP-REQ-026` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_members_func_from_non_creator | — | `SHARED-GROUP-REQ-007` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_add_member_conflict | — | `GTEST-REQ-004` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_create_from_welcome_validation | native; plain test via cfg_attr | `GTEST-REQ-005` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_dm_stitching | — | `SHARED-GROUP-REQ-002` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_add_inbox | — | `SHARED-GROUP-REQ-011` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_create_group_with_member_two_installations_one_malformed_keypackage | native; tokio current_thread | `GTEST-REQ-008` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_create_group_with_member_all_malformed_installations | native; tokio current_thread | `GTEST-REQ-009` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_dm_creation_with_user_two_installations_one_malformed | native; tokio current_thread | `GTEST-REQ-008` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_dm_creation_with_user_all_malformed_installations | native; tokio current_thread | `GTEST-REQ-009` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_add_inbox_with_bad_installation_to_group | native; tokio current_thread | `GTEST-REQ-008` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_add_inbox_with_good_installation_to_group_with_bad_installation | native; tokio current_thread | `GTEST-REQ-010` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_remove_inbox_with_good_installation_from_group_with_bad_installation | native; tokio current_thread | `GTEST-REQ-010` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_remove_inbox_with_bad_installation_from_group | native; tokio current_thread | `GTEST-REQ-011` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_add_invalid_member | — | `SHARED-GROUP-REQ-012` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_add_unregistered_member | — | `SHARED-GROUP-REQ-012` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_remove_inbox | — | `SHARED-GROUP-REQ-011` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_self_remove_dm_must_fail | — | `SHARED-GROUP-REQ-006`, `GTEST-REQ-016` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_self_remove_group_fail_with_one_member | current_thread | `GTEST-REQ-016` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_self_remove_super_admin_must_fail | current_thread | `GTEST-REQ-016` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_non_member_cannot_leave_group | current_thread | `GTEST-REQ-016` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_self_removal | current_thread | `SHARED-GROUP-REQ-014`, `GTEST-REQ-020` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_self_removal_simple | current_thread | `SHARED-GROUP-REQ-014` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_membership_state_after_readd | current_thread | `SHARED-GROUP-REQ-015` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_self_removal_group_update_message | current_thread | `SHARED-GROUP-REQ-016` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_self_removal_single_installations | current_thread | `SHARED-GROUP-REQ-014` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_self_removal_with_multiple_initial_installations | current_thread | `GTEST-REQ-020` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_self_removal_with_late_installation | ignored; current_thread | `GTEST-REQ-020` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_clean_pending_remove_list_on_member_removal | current_thread | `SHARED-GROUP-REQ-016` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_super_admin_promotion_marks_pending_leave_requests | current_thread | `GTEST-REQ-022` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_super_admin_demotion_clears_pending_leave_requests | current_thread | `GTEST-REQ-022` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_no_status_change_when_not_in_pending_remove_list | current_thread | `GTEST-REQ-022` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_promotion_excludes_self_from_pending_check | current_thread | `GTEST-REQ-022` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_admin_removal_without_pending_shows_as_removed | current_thread | `SHARED-GROUP-REQ-016` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_key_update | — | `GTEST-REQ-023` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_post_commit | — | `GTEST-REQ-024` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_remove_by_account_address | — | `SHARED-GROUP-REQ-011` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_removed_members_cannot_send_message_to_others | — | `SHARED-GROUP-REQ-013` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_add_missing_installations | — | `GTEST-REQ-027` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_self_resolve_epoch_mismatch | multi_thread | `GTEST-REQ-028` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_group_permissions | — | `SHARED-GROUP-REQ-020` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_group_options | — | `SHARED-GROUP-REQ-008` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_max_limit_add | ignored | `GTEST-REQ-031` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_group_mutable_data | group-name default; creator update converges locally and to peer; ordinary-member update is rejected and state stays unchanged | `SHARED-GROUP-REQ-009`, `SHARED-GROUP-REQ-020` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_update_policies_empty_group | — | `SHARED-GROUP-REQ-009` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_update_group_image_url_square | — | `SHARED-GROUP-REQ-009` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_update_group_message_expiration_settings | current_thread | `SHARED-GROUP-REQ-025` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_group_mutable_data_group_permissions | current_thread | `SHARED-GROUP-REQ-020` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_group_admin_list_update | — | `SHARED-GROUP-REQ-017` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_group_super_admin_list_update | — | `SHARED-GROUP-REQ-017` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_group_members_permission_level_update | — | `SHARED-GROUP-REQ-017` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_staged_welcome | — | `SHARED-GROUP-REQ-010` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_can_read_group_creator_inbox_id | — | `SHARED-GROUP-REQ-007` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_can_update_gce_after_failed_commit | default-policy group; two successful name updates around an admin-list attempt whose result is not asserted | `GTEST-REQ-041` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_can_update_permissions_after_group_creation | — | `SHARED-GROUP-REQ-020` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_optimistic_send | — | `SHARED-CONTENT-REQ-002` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_dm_creation | — | `SHARED-GROUP-REQ-006` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::process_messages_abort_on_retryable_error | — | `GTEST-REQ-044` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::skip_already_processed_messages | — | `GTEST-REQ-045` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::skip_already_processed_intents | — | `GTEST-REQ-046` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_parallel_syncs | multi_thread | `GTEST-REQ-047` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::add_missing_installs_reentrancy | rstest; multi_thread; wasm ignored | `GTEST-REQ-048` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::respect_allow_epoch_increment | multi_thread | `GTEST-REQ-049` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_get_and_set_consent | rstest and awt; 3 async fixtures; wasm ignored; Denied is read back immediately, not after persistence restart | `SHARED-GROUP-REQ-021` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_max_past_epochs | — | `GTEST-REQ-051` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_validate_dm_group | — | `GTEST-REQ-052` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_respects_character_limits_for_group_metadata | — | `GTEST-REQ-053` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_update_app_data | — | `GTEST-REQ-054` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_app_data_in_dm | — | `GTEST-REQ-055` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_create_group_with_app_data | — | `SHARED-GROUP-REQ-008` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_create_group_with_default_app_data | — | `SHARED-GROUP-REQ-008` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_increment_patch_version | synchronous custom test | `GTEST-REQ-057` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_can_set_min_supported_protocol_version_for_commit | — | `GTEST-REQ-058` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_client_on_old_version_pauses_after_joining_min_version_group | — | `GTEST-REQ-059` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_only_super_admins_can_set_min_supported_protocol_version | — | `GTEST-REQ-060` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_send_message_while_paused_after_welcome_returns_expected_error | — | `GTEST-REQ-061` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_send_message_after_min_version_update_gets_expected_error | — | `GTEST-REQ-061` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_can_make_inbox_with_a_bad_key_package_an_admin | native; tokio multi_thread | `GTEST-REQ-012` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_when_processing_message_return_future_wrong_epoch_group_marked_probably_forked | native; tokio multi_thread | `GTEST-REQ-062` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::can_stream_out_of_order_without_forking | multi_thread | `GTEST-REQ-063` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::own_message_without_intent_skips_and_increments_cursor | multi_thread | `GTEST-REQ-064` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_generate_commit_with_rollback | — | `GTEST-REQ-065` |
| crates/xmtp_mls/src/groups/tests/mod.rs | groups::tests::test_membership_state | — | `GTEST-REQ-066` |
| crates/xmtp_mls/src/groups/tests/test_change_callbacks.rs | groups::tests::test_change_callbacks::test_app_data_callback_fires_for_remote_change | — | `GTEST-REQ-067` |
| crates/xmtp_mls/src/groups/tests/test_change_callbacks.rs | groups::tests::test_change_callbacks::test_app_data_callback_fires_for_local_change | — | `GTEST-REQ-067` |
| crates/xmtp_mls/src/groups/tests/test_change_callbacks.rs | groups::tests::test_change_callbacks::test_app_data_callback_silent_for_unrelated_changes | — | `GTEST-REQ-067` |
| crates/xmtp_mls/src/groups/tests/test_change_callbacks.rs | groups::tests::test_change_callbacks::test_callback_can_publish_back_into_the_same_group | timeout in body | `GTEST-REQ-069` |
| crates/xmtp_mls/src/groups/tests/test_change_callbacks.rs | groups::tests::test_change_callbacks::test_pending_local_intent_clobbers_a_remote_change | — | `GTEST-REQ-070` |
| crates/xmtp_mls/src/groups/tests/test_change_callbacks.rs | groups::tests::test_change_callbacks::test_guarded_update_is_abandoned_instead_of_clobbering | — | `GTEST-REQ-071` |
| crates/xmtp_mls/src/groups/tests/test_change_callbacks.rs | groups::tests::test_change_callbacks::test_guarded_update_reports_the_value_that_landed | — | `GTEST-REQ-072` |
| crates/xmtp_mls/src/groups/tests/test_change_callbacks.rs | groups::tests::test_change_callbacks::test_superseded_intent_resolves_promptly_as_an_error | — | `GTEST-REQ-073` |
| crates/xmtp_mls/src/groups/tests/test_change_callbacks.rs | groups::tests::test_change_callbacks::test_wedged_callback_does_not_stall_sync_forever | 500 ms callback timeout | `GTEST-REQ-074` |
| crates/xmtp_mls/src/groups/tests/test_change_callbacks.rs | groups::tests::test_change_callbacks::test_wedged_callback_drops_the_rest_of_its_batch | 500 ms callback timeout | `GTEST-REQ-075` |
| crates/xmtp_mls/src/groups/tests/test_commit_log_fork_detection.rs | groups::tests::test_commit_log_fork_detection::test_commit_log_fork_detection_no_fork | d14n wasm ignored; matching result and authenticator | `GTEST-REQ-076` |
| crates/xmtp_mls/src/groups/tests/test_commit_log_fork_detection.rs | groups::tests::test_commit_log_fork_detection::test_commit_log_fork_detection_forked | d14n wasm ignored; combined result mismatch and different authenticator; causes are not isolated | `GTEST-REQ-076` |
| crates/xmtp_mls/src/groups/tests/test_commit_log_fork_detection.rs | groups::tests::test_commit_log_fork_detection::test_commit_log_fork_detection_cursor_updates | d14n wasm ignored | `GTEST-REQ-077` |
| crates/xmtp_mls/src/groups/tests/test_commit_log_fork_detection.rs | groups::tests::test_commit_log_fork_detection::test_commit_log_fork_detection_returns_none_when_no_matching_remote | d14n wasm ignored | `GTEST-REQ-078` |
| crates/xmtp_mls/src/groups/tests/test_commit_log_fork_detection.rs | groups::tests::test_commit_log_fork_detection::test_commit_log_fork_status_persistence_no_new_commits | d14n wasm ignored | `GTEST-REQ-079` |
| crates/xmtp_mls/src/groups/tests/test_commit_log_fork_detection.rs | groups::tests::test_commit_log_fork_detection::test_fork_detection_not_triggered_by_removal_and_readd | d14n wasm ignored | `GTEST-REQ-080` |
| crates/xmtp_mls/src/groups/tests/test_commit_log_fork_detection.rs | groups::tests::test_commit_log_fork_detection::test_merge_staged_commit_logged_rejects_non_advancing_authenticator | native mechanics; d14n wasm ignored | `GTEST-REQ-081` |
| crates/xmtp_mls/src/groups/tests/test_commit_log_local.rs | groups::tests::test_commit_log_local::test_successful_commit_log_types | d14n wasm ignored | `GTEST-REQ-082` |
| crates/xmtp_mls/src/groups/tests/test_commit_log_local.rs | groups::tests::test_commit_log_local::test_failed_application_message_not_added_to_commit_log | d14n wasm ignored | `GTEST-REQ-083` |
| crates/xmtp_mls/src/groups/tests/test_commit_log_local.rs | groups::tests::test_commit_log_local::test_welcome_commit_log | d14n wasm ignored | `GTEST-REQ-084` |
| crates/xmtp_mls/src/groups/tests/test_commit_log_local.rs | groups::tests::test_commit_log_local::test_commit_log_retriable_error | ignored; toxiproxy | `GTEST-REQ-085` |
| crates/xmtp_mls/src/groups/tests/test_commit_log_local.rs | groups::tests::test_commit_log_local::test_commit_log_non_retriable_error | d14n wasm ignored | `GTEST-REQ-086` |
| crates/xmtp_mls/src/groups/tests/test_commit_log_readd_requests.rs | groups::tests::test_commit_log_readd_requests::test_request_readd | d14n wasm ignored; Boolean awaiting-readd checks after two worker ticks; no send or count assertion | `GTEST-REQ-087` |
| crates/xmtp_mls/src/groups/tests/test_commit_log_readd_requests.rs | groups::tests::test_commit_log_readd_requests::test_request_readd_dm | d14n wasm ignored; DM Boolean awaiting-readd checks after two worker ticks; no send or count assertion | `GTEST-REQ-087` |
| crates/xmtp_mls/src/groups/tests/test_commit_log_readd_requests.rs | groups::tests::test_commit_log_readd_requests::test_readd_installation_succeeds | d14n wasm ignored | `GTEST-REQ-088` |
| crates/xmtp_mls/src/groups/tests/test_commit_log_readd_requests.rs | groups::tests::test_commit_log_readd_requests::test_readd_bookkeeping | d14n wasm ignored | `GTEST-REQ-089` |
| crates/xmtp_mls/src/groups/tests/test_commit_log_readd_requests.rs | groups::tests::test_commit_log_readd_requests::test_request_readd_with_allowlisted_groups | d14n wasm ignored | `GTEST-REQ-090` |
| crates/xmtp_mls/src/groups/tests/test_commit_log_remote.rs | groups::tests::test_commit_log_remote::test_commit_log_signer_on_group_creation | d14n wasm ignored | `GTEST-REQ-091` |
| crates/xmtp_mls/src/groups/tests/test_commit_log_remote.rs | groups::tests::test_commit_log_remote::test_device_sync_mutable_metadata_is_overwritten | d14n wasm ignored | `GTEST-REQ-092` |
| crates/xmtp_mls/src/groups/tests/test_commit_log_remote.rs | groups::tests::test_commit_log_remote::test_commit_log_publish_and_query_apis | d14n wasm ignored | `GTEST-REQ-093` |
| crates/xmtp_mls/src/groups/tests/test_commit_log_remote.rs | groups::tests::test_commit_log_remote::test_should_publish_commit_log | d14n wasm ignored; compares creator Alix with normal joiner Bo; no promoted non-creator super-admin case | `GTEST-REQ-094` |
| crates/xmtp_mls/src/groups/tests/test_commit_log_remote.rs | groups::tests::test_commit_log_remote::test_publish_commit_log_to_remote | d14n wasm ignored | `GTEST-REQ-096` |
| crates/xmtp_mls/src/groups/tests/test_commit_log_remote.rs | groups::tests::test_commit_log_remote::test_download_commit_log_from_remote | d14n wasm ignored | `GTEST-REQ-096` |
| crates/xmtp_mls/src/groups/tests/test_commit_log_remote.rs | groups::tests::test_commit_log_remote::test_should_skip_remote_log_entry | d14n wasm ignored | `GTEST-REQ-097` |
| crates/xmtp_mls/src/groups/tests/test_commit_log_remote.rs | groups::tests::test_commit_log_remote::test_all_users_use_same_signing_key_for_publishing | d14n wasm ignored | `GTEST-REQ-098` |
| crates/xmtp_mls/src/groups/tests/test_commit_log_remote.rs | groups::tests::test_commit_log_remote::test_consecutive_entries_verification_happy_case | d14n wasm ignored | `GTEST-REQ-099` |
| crates/xmtp_mls/src/groups/tests/test_commit_log_remote.rs | groups::tests::test_commit_log_remote::test_bad_signature_handling | d14n wasm ignored | `GTEST-REQ-100` |
| crates/xmtp_mls/src/groups/tests/test_commit_log_remote.rs | groups::tests::test_commit_log_remote::test_update_commit_log_signer_sync_across_parties | d14n wasm ignored | `GTEST-REQ-101` |
| crates/xmtp_mls/src/groups/tests/test_commit_log_remote.rs | groups::tests::test_commit_log_remote::test_updating_group_name_preserves_commit_log_signer | d14n wasm ignored | `GTEST-REQ-102` |
| crates/xmtp_mls/src/groups/tests/test_commit_log_remote.rs | groups::tests::test_commit_log_remote::test_legacy_group_signing_key_discovery_via_remote_commit_log | d14n wasm ignored | `GTEST-REQ-103` |
| crates/xmtp_mls/src/groups/tests/test_consent.rs | groups::tests::test_consent::test_auto_consent_to_own_group | — | `GTEST-REQ-104` |
| crates/xmtp_mls/src/groups/tests/test_delete_message.rs | groups::tests::test_delete_message::test_delete_message_by_sender | — | `GTEST-REQ-108` |
| crates/xmtp_mls/src/groups/tests/test_delete_message.rs | groups::tests::test_delete_message::test_delete_message_by_super_admin | — | `GTEST-REQ-109`, `GTEST-REQ-118` |
| crates/xmtp_mls/src/groups/tests/test_delete_message.rs | groups::tests::test_delete_message::test_delete_message_authorization_failure | — | `GTEST-REQ-110` |
| crates/xmtp_mls/src/groups/tests/test_delete_message.rs | groups::tests::test_delete_message::test_cannot_delete_transcript_messages | — | `SHARED-CONTENT-REQ-005` |
| crates/xmtp_mls/src/groups/tests/test_delete_message.rs | groups::tests::test_delete_message::test_delete_nonexistent_message | — | `GTEST-REQ-112` |
| crates/xmtp_mls/src/groups/tests/test_delete_message.rs | groups::tests::test_delete_message::test_delete_already_deleted_message | — | `GTEST-REQ-112` |
| crates/xmtp_mls/src/groups/tests/test_delete_message.rs | groups::tests::test_delete_message::test_out_of_order_deletion | — | `GTEST-REQ-113` |
| crates/xmtp_mls/src/groups/tests/test_delete_message.rs | groups::tests::test_delete_message::test_true_out_of_order_deletion_by_sender | direct database fixture | `GTEST-REQ-113` |
| crates/xmtp_mls/src/groups/tests/test_delete_message.rs | groups::tests::test_delete_message::test_out_of_order_unauthorized_deletion_rejected | direct malicious database fixture | `GTEST-REQ-113` |
| crates/xmtp_mls/src/groups/tests/test_delete_message.rs | groups::tests::test_delete_message::test_enrichment_with_deleted_messages | — | `GTEST-REQ-115` |
| crates/xmtp_mls/src/groups/tests/test_delete_message.rs | groups::tests::test_delete_message::test_delete_message_filtered_from_lists | — | `SHARED-CONTENT-REQ-007` |
| crates/xmtp_mls/src/groups/tests/test_delete_message.rs | groups::tests::test_delete_message::test_deletion_database_queries | — | `SHARED-CONTENT-REQ-006` |
| crates/xmtp_mls/src/groups/tests/test_delete_message.rs | groups::tests::test_delete_message::test_admin_deletion_flag | — | `GTEST-REQ-118` |
| crates/xmtp_mls/src/groups/tests/test_delete_message.rs | groups::tests::test_delete_message::test_reply_to_deleted_message | — | `GTEST-REQ-115` |
| crates/xmtp_mls/src/groups/tests/test_delete_message.rs | groups::tests::test_delete_message::test_cannot_delete_message_from_different_group | — | `SHARED-CONTENT-REQ-004` |
| crates/xmtp_mls/src/groups/tests/test_delete_message.rs | groups::tests::test_delete_message::test_cannot_delete_delete_message | — | `SHARED-CONTENT-REQ-005` |
| crates/xmtp_mls/src/groups/tests/test_delete_message.rs | groups::tests::test_delete_message::test_concurrent_deletions | sequential conflict simulation | `GTEST-REQ-120` |
| crates/xmtp_mls/src/groups/tests/test_delete_message.rs | groups::tests::test_delete_message::test_sender_and_admin_both_delete | — | `GTEST-REQ-120` |
| crates/xmtp_mls/src/groups/tests/test_delete_message.rs | groups::tests::test_delete_message::test_out_of_order_sender_deletion_shows_correct_deleted_by | direct database fixture | `GTEST-REQ-118` |
| crates/xmtp_mls/src/groups/tests/test_delete_message.rs | groups::tests::test_delete_message::test_stream_message_deletions_from_other_client | callback stream; 5 s timeout | `GTEST-REQ-122` |
| crates/xmtp_mls/src/groups/tests/test_delete_message.rs | groups::tests::test_delete_message::test_stream_message_deletions_fires_for_self_after_publish | callback stream; 5 s timeout | `GTEST-REQ-122` |
| crates/xmtp_mls/src/groups/tests/test_dm.rs | groups::tests::test_dm::auto_consent_dms_for_new_installations | — | `GTEST-REQ-105` |
| crates/xmtp_mls/src/groups/tests/test_dm.rs | groups::tests::test_dm::test_dm_welcome_with_preexisting_consent | — | `GTEST-REQ-106` |
| crates/xmtp_mls/src/groups/tests/test_dm.rs | groups::tests::test_dm::test_group_update_dedupes | — | `GTEST-REQ-107` |
| crates/xmtp_mls/src/groups/tests/test_extract_readded_installations.rs | groups::tests::test_extract_readded_installations::test_extract_readded_installations_non_super_admin_returns_empty | plain test | `GTEST-REQ-123` |
| crates/xmtp_mls/src/groups/tests/test_extract_readded_installations.rs | groups::tests::test_extract_readded_installations::test_extract_readded_installations_super_admin_added_and_removed_intersection | plain test | `GTEST-REQ-123` |
| crates/xmtp_mls/src/groups/tests/test_extract_readded_installations.rs | groups::tests::test_extract_readded_installations::test_extract_readded_installations_super_admin_failed_and_removed_intersection | plain test | `GTEST-REQ-123` |
| crates/xmtp_mls/src/groups/tests/test_extract_readded_installations.rs | groups::tests::test_extract_readded_installations::test_extract_readded_installations_super_admin_both_types_of_readd | plain test | `GTEST-REQ-123` |
| crates/xmtp_mls/src/groups/tests/test_extract_readded_installations.rs | groups::tests::test_extract_readded_installations::test_extract_readded_installations_super_admin_no_intersections | plain test | `GTEST-REQ-123` |
| crates/xmtp_mls/src/groups/tests/test_extract_readded_installations.rs | groups::tests::test_extract_readded_installations::test_extract_readded_installations_super_admin_empty_sets | plain test | `GTEST-REQ-123` |
| crates/xmtp_mls/src/groups/tests/test_extract_readded_installations.rs | groups::tests::test_extract_readded_installations::test_extract_readded_installations_super_admin_all_installations_readded | plain test | `GTEST-REQ-123` |
| crates/xmtp_mls/src/groups/tests/test_extract_readded_installations.rs | groups::tests::test_extract_readded_installations::test_extract_readded_installations_super_admin_multiple_failed_intersections | plain test | `GTEST-REQ-123` |
| crates/xmtp_mls/src/groups/tests/test_extract_readded_installations.rs | groups::tests::test_extract_readded_installations::test_extract_readded_installations_super_admin_overlapping_scenarios | plain test | `GTEST-REQ-123` |
| crates/xmtp_mls/src/groups/tests/test_failed_installations.rs | groups::tests::test_failed_installations::publish_time_key_package_failure_lands_in_membership | native module | `GTEST-REQ-124` |
| crates/xmtp_mls/src/groups/tests/test_failed_installations.rs | groups::tests::test_failed_installations::joiner_accepts_welcome_with_publish_time_failed_installation | native module | `GTEST-REQ-125` |
| crates/xmtp_mls/src/groups/tests/test_group_updated.rs | groups::tests::test_group_updated::test_group_updated_admin_changes | — | `GTEST-REQ-126` |
| crates/xmtp_mls/src/groups/tests/test_libxmtp_version.rs | groups::tests::test_libxmtp_version::test_parse_and_compare_basic_versions | plain test | `GTEST-REQ-127` |
| crates/xmtp_mls/src/groups/tests/test_libxmtp_version.rs | groups::tests::test_libxmtp_version::test_parse_and_compare_with_suffixes | plain test | `GTEST-REQ-128` |
| crates/xmtp_mls/src/groups/tests/test_libxmtp_version.rs | groups::tests::test_libxmtp_version::test_parse_and_compare_zero_versions | plain test | `GTEST-REQ-127` |
| crates/xmtp_mls/src/groups/tests/test_libxmtp_version.rs | groups::tests::test_libxmtp_version::test_numeric_pre_release_identifiers_compare_numerically | plain test | `GTEST-REQ-128` |
| crates/xmtp_mls/src/groups/tests/test_libxmtp_version.rs | groups::tests::test_libxmtp_version::test_multi_segment_pre_release_parses | plain test | `GTEST-REQ-129` |
| crates/xmtp_mls/src/groups/tests/test_libxmtp_version.rs | groups::tests::test_libxmtp_version::test_build_metadata_parses | plain test | `GTEST-REQ-129` |
| crates/xmtp_mls/src/groups/tests/test_libxmtp_version.rs | groups::tests::test_libxmtp_version::test_parse_invalid_format | plain test; loop has 6 inputs | `GTEST-REQ-130` |
| crates/xmtp_mls/src/groups/tests/test_libxmtp_version.rs | groups::tests::test_libxmtp_version::proposals_min_protocol_version_does_not_exceed_workspace_version | plain test | `GTEST-REQ-131` |
| crates/xmtp_mls/src/groups/tests/test_message_dependencies.rs | groups::tests::test_message_dependencies::messages_have_dependencies | d14n-only module | `GTEST-REQ-132` |
| crates/xmtp_mls/src/groups/tests/test_message_dependencies.rs | groups::tests::test_message_dependencies::messages_dependencies_out_of_order_invites | d14n-only module | `GTEST-REQ-132` |
| crates/xmtp_mls/src/groups/tests/test_message_disappearing_settings.rs | groups::tests::test_message_disappearing_settings::test_disappearing_message_update_message_in_group | — | `GTEST-REQ-107` |
| crates/xmtp_mls/src/groups/tests/test_metadata_read_amplification.rs | groups::tests::test_metadata_read_amplification::metadata_read_amplification | native module | `GTEST-REQ-134` |
| crates/xmtp_mls/src/groups/tests/test_network.rs | groups::tests::test_network::test_bad_network | native module; toxiproxy | `GTEST-REQ-135` |
| crates/xmtp_mls/src/groups/tests/test_prepare_message_for_later_publish.rs | groups::tests::test_prepare_message_for_later_publish::test_prepare_message_stores_unpublished | — | `GTEST-REQ-136` |
| crates/xmtp_mls/src/groups/tests/test_prepare_message_for_later_publish.rs | groups::tests::test_prepare_message_for_later_publish::test_publish_messages_does_not_publish_prepared_messages | — | `GTEST-REQ-136` |
| crates/xmtp_mls/src/groups/tests/test_prepare_message_for_later_publish.rs | groups::tests::test_prepare_message_for_later_publish::test_publish_stored_message_publishes_prepared_message | — | `GTEST-REQ-136` |
| crates/xmtp_mls/src/groups/tests/test_prepare_message_for_later_publish.rs | groups::tests::test_prepare_message_for_later_publish::test_publish_stored_message_is_idempotent | three publish calls | `GTEST-REQ-136` |
| crates/xmtp_mls/src/groups/tests/test_prepare_message_for_later_publish.rs | groups::tests::test_prepare_message_for_later_publish::test_selective_publish_of_prepared_messages | publish 2 of 3 | `GTEST-REQ-136` |
| crates/xmtp_mls/src/groups/tests/test_prepare_message_for_later_publish.rs | groups::tests::test_prepare_message_for_later_publish::test_explicit_idempotency_key_produces_deterministic_id | — | `SHARED-CONTENT-REQ-003` |
| crates/xmtp_mls/src/groups/tests/test_prepare_message_for_later_publish.rs | groups::tests::test_prepare_message_for_later_publish::test_default_idempotency_key_is_unique_per_send | — | `GTEST-REQ-139` |
| crates/xmtp_mls/src/groups/tests/test_prepare_message_for_later_publish.rs | groups::tests::test_prepare_message_for_later_publish::test_duplicate_idempotency_key_is_idempotent | duplicate prepare | `SHARED-CONTENT-REQ-003` |
| crates/xmtp_mls/src/groups/tests/test_prepare_message_for_later_publish.rs | groups::tests::test_prepare_message_for_later_publish::test_idempotency_key_crosses_the_wire | — | `SHARED-CONTENT-REQ-003` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_all_members_support_proposals_consistency | rstest; cases 0, 1, 2, 4 additional members | `GTEST-REQ-143` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_proposal_intent_serialization | rstest; 6 add and remove cases | `GTEST-REQ-144` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_proposals_enabled_default_false | — | `GTEST-REQ-145` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_e2e_propose_add_member_flow | — | `GTEST-REQ-146` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_e2e_propose_remove_member_flow | — | `GTEST-REQ-147` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_commit_with_no_pending_proposals | — | `GTEST-REQ-148` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_propose_invalid_member_operations | rstest; add-existing and remove-absent | `GTEST-REQ-149` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_message_auto_commits_pending_proposals | — | `GTEST-REQ-150` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_multiple_add_proposals_before_commit | — | `GTEST-REQ-151` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_mixed_add_remove_proposals_before_commit | — | `GTEST-REQ-152` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_propose_group_context_extensions_intent | — | `GTEST-REQ-144` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_proposer_can_commit_own_proposal | — | `GTEST-REQ-154` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_concurrent_proposals_from_different_members | — | `GTEST-REQ-155` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_enable_proposals_concurrent_callers_converge | concurrent tokio join | `GTEST-REQ-156` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_non_admin_proposal_rejected_in_admin_only_group | — | `GTEST-REQ-157` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_admin_proposal_accepted_in_admin_only_group | — | `GTEST-REQ-158` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_enable_proposals_and_proposals_enabled | — | `GTEST-REQ-159` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_enable_proposals_fails_without_support | task-local capability off | `GTEST-REQ-160` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_adding_unsupported_member_rejected_when_proposals_enabled | task-local capability off | `GTEST-REQ-161` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_enable_proposals_rejects_min_version_above_own | — | `GTEST-REQ-162` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_update_group_min_version_rejects_downgrade | — | `GTEST-REQ-163` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_enable_proposals_idempotent_with_forward_min_version | — | `GTEST-REQ-164` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_update_group_min_version_rejects_malformed_input | — | `GTEST-REQ-165` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_update_group_min_version_rejects_above_own | — | `GTEST-REQ-166` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_build_extensions_for_membership_update | — | `GTEST-REQ-167` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_non_admin_commits_admin_proposals_in_admin_group | — | `GTEST-REQ-168` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_multiple_non_admin_proposers_with_admin_committer | — | `GTEST-REQ-168` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_remove_proposal_validation_in_admin_group | two targets | `GTEST-REQ-169` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_admin_proposes_remove_committed_by_non_admin | — | `GTEST-REQ-168` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_non_admin_gce_metadata_proposal_rejected | two invalid GCE shapes | `GTEST-REQ-170` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_non_admin_gce_admin_list_proposal_rejected | three role-list operations | `GTEST-REQ-171` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_non_super_admin_gce_permission_change_rejected | — | `GTEST-REQ-172` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_add_members_batched_when_proposals_enabled | — | `GTEST-REQ-173` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_add_members_direct_commit_when_proposals_disabled | — | `GTEST-REQ-174` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_commit_pending_proposals_batches_gce_and_commit | — | `GTEST-REQ-175` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_sequence_id_bump_triggers_gce_with_proposals_enabled | — | `GTEST-REQ-176` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_add_member_after_sequence_id_bump_with_proposals_enabled | — | `GTEST-REQ-177` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_app_data_dictionary_capability_and_required | pre and post migration | `GTEST-REQ-178` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_app_data_update_advertised_but_not_required | — | `GTEST-REQ-179` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_key_package_rotation_preserves_app_data_dictionary_capability | before and after rotation | `GTEST-REQ-180` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_update_group_name_via_app_data_update | — | `GTEST-REQ-181` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_update_group_description_via_app_data_update | — | `GTEST-REQ-181` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_disappearing_settings_survive_bootstrap | pre and post migration | `GTEST-REQ-182` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_enable_proposals_pauses_old_client_via_legacy_gmm_bump | cross-version | `GTEST-REQ-183` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_update_group_name_uses_legacy_path_when_proposals_disabled | — | `GTEST-REQ-184` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_inline_app_data_update_denied_by_registry_policy | typed failure summary | `GTEST-REQ-185` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_accumulate_app_data_updates_chains_intra_batch | pure accumulator seam | `GTEST-REQ-186` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_admin_list_add_via_app_data_path_after_migration | — | `GTEST-REQ-187` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_admin_list_remove_via_app_data_path_after_migration | — | `GTEST-REQ-187` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_super_admin_list_add_via_app_data_path_after_migration | — | `GTEST-REQ-187` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_permission_update_via_app_data_path_after_migration | — | `GTEST-REQ-188` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_admin_list_add_unchanged_on_unmigrated_group | — | `GTEST-REQ-189` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_downgraded_client_pauses_at_bootstrap_seeding_higher_floor | persistent-store downgrade | `GTEST-REQ-190` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_welcome_on_migrated_group_pauses_below_min_version | cross-version welcome | `GTEST-REQ-191` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_steady_state_pause_on_min_version_bump_via_app_data_update | cross-version migrated commit | `GTEST-REQ-192` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_downgraded_client_pauses_on_migrated_group_with_higher_floor | persistent-store restart | `GTEST-REQ-193` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_unstick_paused_groups_recovers_after_upgrade | four stored-floor states | `GTEST-REQ-194` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_enable_proposals_no_wire_commit_on_already_migrated | repeated calls | `GTEST-REQ-195` |
| crates/xmtp_mls/src/groups/tests/test_proposals.rs | groups::tests::test_proposals::test_membership_capabilities | pre and post migration | `GTEST-REQ-196` |
| crates/xmtp_mls/src/groups/tests/test_send_message_opts.rs | groups::tests::test_send_message_opts::test_send_message_should_push | true and false | `GTEST-REQ-141` |
| crates/xmtp_mls/src/groups/tests/test_starting_membership_sequence_id.rs | groups::tests::test_starting_membership_sequence_id::metadata_update_on_freshly_created_group_succeeds | — | `GTEST-REQ-142` |
| crates/xmtp_mls/src/groups/tests/test_starting_membership_sequence_id.rs | groups::tests::test_starting_membership_sequence_id::key_update_on_freshly_created_group_succeeds | — | `GTEST-REQ-142` |
| crates/xmtp_mls/src/groups/tests/test_validate_app_data_update.rs | groups::tests::test_validate_app_data_update::bytes_update_allowed_when_registry_allows | plain test | `GTEST-REQ-197` |
| crates/xmtp_mls/src/groups/tests/test_validate_app_data_update.rs | groups::tests::test_validate_app_data_update::bytes_update_accepts_none_old_value_for_first_write | plain test | `GTEST-REQ-197` |
| crates/xmtp_mls/src/groups/tests/test_validate_app_data_update.rs | groups::tests::test_validate_app_data_update::bytes_remove_allowed_when_registry_allows_delete | plain test | `GTEST-REQ-197` |
| crates/xmtp_mls/src/groups/tests/test_validate_app_data_update.rs | groups::tests::test_validate_app_data_update::bytes_update_rejected_when_registry_empty | plain test | `GTEST-REQ-197` |
| crates/xmtp_mls/src/groups/tests/test_validate_app_data_update.rs | groups::tests::test_validate_app_data_update::bytes_update_rejected_when_policy_denies | plain test | `GTEST-REQ-197` |
| crates/xmtp_mls/src/groups/tests/test_validate_app_data_update.rs | groups::tests::test_validate_app_data_update::admin_list_insert_rejected_for_member | plain test | `GTEST-REQ-197` |
| crates/xmtp_mls/src/groups/tests/test_validate_app_data_update.rs | groups::tests::test_validate_app_data_update::super_admin_list_insert_rejected_for_admin | plain test | `GTEST-REQ-197` |
| crates/xmtp_mls/src/groups/tests/test_validate_app_data_update.rs | groups::tests::test_validate_app_data_update::malformed_delta_maps_to_insufficient_permissions | plain test | `GTEST-REQ-197` |
| crates/xmtp_mls/src/groups/tests/test_validate_app_data_update.rs | groups::tests::test_validate_app_data_update::unknown_collection_component_maps_to_insufficient_permissions | plain test | `GTEST-REQ-197` |
| crates/xmtp_mls/src/groups/tests/test_validate_app_data_update.rs | groups::tests::test_validate_app_data_update::remove_by_hash_miss_does_not_short_circuit_policy | plain test | `GTEST-REQ-197` |
| crates/xmtp_mls/src/groups/tests/test_validate_app_data_update.rs | groups::tests::test_validate_app_data_update::multi_mutation_delta_all_allowed_returns_ok | plain test | `GTEST-REQ-197` |
| crates/xmtp_mls/src/groups/tests/test_validate_app_data_update.rs | groups::tests::test_validate_app_data_update::proposer_leaf_member_returns_leaf_index | plain test | `GTEST-REQ-197` |
| crates/xmtp_mls/src/groups/tests/test_validate_app_data_update.rs | groups::tests::test_validate_app_data_update::proposer_leaf_external_rejected_as_actor_not_member | plain test | `GTEST-REQ-197` |
| crates/xmtp_mls/src/groups/tests/test_validate_app_data_update.rs | groups::tests::test_validate_app_data_update::proposer_leaf_new_member_commit_rejected | plain test | `GTEST-REQ-197` |
| crates/xmtp_mls/src/groups/tests/test_validate_app_data_update.rs | groups::tests::test_validate_app_data_update::proposer_leaf_new_member_proposal_rejected | plain test | `GTEST-REQ-197` |
| crates/xmtp_mls/src/groups/tests/test_validate_app_data_update.rs | groups::tests::test_validate_app_data_update::unknown_component_in_xmtp_range_rejected_without_registry_entry | plain test | `GTEST-REQ-197` |
| crates/xmtp_mls/src/groups/tests/test_validate_app_data_update.rs | groups::tests::test_validate_app_data_update::unknown_component_in_xmtp_range_allowed_when_registry_permits | plain test | `GTEST-REQ-197` |
| crates/xmtp_mls/src/groups/tests/test_validate_app_data_update.rs | groups::tests::test_validate_app_data_update::unknown_component_in_app_range_allowed_when_registry_permits | plain test | `GTEST-REQ-197` |
| crates/xmtp_mls/src/groups/tests/test_validate_app_data_update.rs | groups::tests::test_validate_app_data_update::unknown_component_in_reserved_range_rejected_with_empty_registry | plain test | `GTEST-REQ-197` |
| crates/xmtp_mls/src/groups/tests/test_validate_app_data_update.rs | groups::tests::test_validate_app_data_update::unknown_component_remove_with_no_prior_rejected_without_registry_entry | plain test | `GTEST-REQ-197` |
| crates/xmtp_mls/src/groups/tests/test_validate_app_data_update.rs | groups::tests::test_validate_app_data_update::unknown_component_remove_allowed_when_registry_permits_delete | plain test | `GTEST-REQ-197` |
| crates/xmtp_mls/src/groups/tests/test_validate_app_data_update.rs | groups::tests::test_validate_app_data_update::unknown_component_update_with_malformed_prior_rejected | plain test | `GTEST-REQ-197` |
| crates/xmtp_mls/src/groups/tests/test_welcome_pointers.rs | groups::tests::test_welcome_pointers::test_welcome_pointer_round_trip_with_welcome_pointers | rstest; 40 s timeout; d14n wasm ignored | `GTEST-REQ-206` |
| crates/xmtp_mls/src/groups/tests/test_welcome_pointers.rs | groups::tests::test_welcome_pointers::test_welcome_pointer_round_trip_without_welcome_pointers | rstest; 80 s timeout; d14n wasm ignored | `GTEST-REQ-206` |
| crates/xmtp_mls/src/groups/tests/test_welcome_pointers.rs | groups::tests::test_welcome_pointers::test_welcome_pointer_round_trip_with_random_mix_of_welcome_pointers | rstest; 40 s timeout; d14n wasm ignored | `GTEST-REQ-206` |
| crates/xmtp_mls/src/groups/tests/test_welcome_pointers.rs | groups::tests::test_welcome_pointers::test_welcome_pointer_encryption_round_trip | plain test | `GTEST-REQ-207` |
| crates/xmtp_mls/src/groups/tests/test_welcome_pointers.rs | groups::tests::test_welcome_pointers::test_welcome_pointer_proto_round_trip | plain test | `GTEST-REQ-208` |
| crates/xmtp_mls/src/groups/tests/test_welcome_pointers.rs | groups::tests::test_welcome_pointers::test_welcome_pointer_resolution_for_no_destination | rstest; 20 s timeout | `GTEST-REQ-209` |
| crates/xmtp_mls/src/groups/tests/test_welcome_pointers.rs | groups::tests::test_welcome_pointers::test_welcome_pointer_resolution_to_another_welcome_pointer | — | `GTEST-REQ-210` |
| crates/xmtp_mls/src/groups/tests/test_welcome_pointers.rs | groups::tests::test_welcome_pointers::test_welcome_pointer_task_retry_resolution | rstest; 40 s timeout | `GTEST-REQ-211` |
| crates/xmtp_mls/src/groups/tests/test_welcomes.rs | groups::tests::test_welcomes::test_welcome_cursor | — | `GTEST-REQ-212` |
| crates/xmtp_mls/src/groups/tests/test_welcomes.rs | groups::tests::test_welcomes::test_inviting_members_results_in_consistent_state | — | `GTEST-REQ-213` |
| crates/xmtp_mls/src/groups/tests/test_welcomes.rs | groups::tests::test_welcomes::test_spoofed_inbox_id | adversarial internal construction | `GTEST-REQ-214` |
