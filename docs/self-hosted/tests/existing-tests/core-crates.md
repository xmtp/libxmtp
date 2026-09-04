# Core archive, cryptography, identity, and database test index

[← Test inventory](../existing-tests.md) · [Requirements](../existing-requirements.md)

- Inventory: 277 source declarations in 49 test-bearing files.
- Count rule: one parameterized declaration is one row. Native and WASM `cfg_attr` expansions do not add rows.
- The four ignored pool declarations each have two rstest cases. The two parameterized refresh-state declarations each have four cases.
- No documentation tests were found in these crates.

| File | Fully qualified test name | Form, gates, and cases | Requirements |
| --- | --- | --- | --- |
| crates/xmtp_archive/src/archive_options.rs | `archive_options::tests::test_element_selection_round_trip` | `#[test]`; active. | `CORE-REQ-001` |
| crates/xmtp_archive/src/archive_options.rs | `archive_options::tests::test_options_round_trip` | `#[test]`; active. | `CORE-REQ-002` |
| crates/xmtp_archive/src/archive_options.rs | `archive_options::tests::test_default` | `#[test]`; active. | `CORE-REQ-003` |
| crates/xmtp_archive/src/archive_options.rs | `archive_options::tests::test_from_ref` | `#[test]`; active. | `CORE-REQ-004` |
| crates/xmtp_archive/src/util.rs | `util::tests::test_generic_array_ext` | `xmtp_common::test`; async declaration; `unwrap_try`. | `CORE-REQ-005` |
| crates/xmtp_cryptography/src/basic_credential.rs | `basic_credential::tests::test_is_binary_compatible_with_mls_deser` | Unit test selected by native or WASM `cfg_attr`. | `CORE-REQ-006` |
| crates/xmtp_cryptography/src/basic_credential.rs | `basic_credential::tests::test_is_binary_compatible_with_mls_ser` | Unit test selected by native or WASM `cfg_attr`. | `CORE-REQ-006` |
| crates/xmtp_cryptography/src/basic_credential.rs | `basic_credential::tests::test_is_binary_compatible_with_mls_deser_serde` | Unit test selected by native or WASM `cfg_attr`. | `CORE-REQ-006` |
| crates/xmtp_cryptography/src/basic_credential.rs | `basic_credential::tests::test_is_binary_compatible_with_mls_ser_serde` | Unit test selected by native or WASM `cfg_attr`. | `CORE-REQ-006` |
| crates/xmtp_cryptography/src/basic_credential.rs | `basic_credential::tests::secret_key_can_not_be_exposed` | Unit test selected by native or WASM `cfg_attr`; checks only three accessor outputs for byte inequality with one secret. | `CORE-REQ-008` |
| crates/xmtp_cryptography/src/ethereum.rs | `ethereum::tests::test_public_key_generation_and_address` | `#[test]`; active. | `CORE-REQ-009` |
| crates/xmtp_cryptography/src/ethereum.rs | `ethereum::tests::test_sign_recoverable_with_known_values` | `#[test]`; active. | `CORE-REQ-010` |
| crates/xmtp_cryptography/src/ethereum.rs | `ethereum::tests::test_hash_personal` | `#[test]`; 32-byte shape, repeat determinism, and different-input inequality; no reference vector. | `CORE-REQ-011` |
| crates/xmtp_cryptography/src/ethereum.rs | `ethereum::tests::test_invalid_inputs` | `#[test]`; zero and max scalar, public-key shape, and short-hash cases. | `CORE-REQ-012` |
| crates/xmtp_cryptography/src/ethereum.rs | `ethereum::tests::test_eip191_hashing_compatibility` | `#[test]`; Alloy compatibility vector. | `CORE-REQ-013` |
| crates/xmtp_cryptography/src/ethereum.rs | `ethereum::tests::test_signature_round_trip_compatibility` | `#[test]`; correct and wrong message cases. | `CORE-REQ-014` |
| crates/xmtp_cryptography/src/ethereum.rs | `ethereum::tests::test_zeroizing_private_key` | `#[test]`; functional zeroizing-wrapper path. | `CORE-REQ-015` |
| crates/xmtp_cryptography/src/signature.rs | `signature::tests::test_eth_address` | Unit test selected by native or WASM `cfg_attr`. | `CORE-REQ-016` |
| crates/xmtp_id/src/key_package/mls_ext_wrapper_encryption.rs | `key_package::mls_ext_wrapper_encryption::tests::test_serialization` | `xmtp_common::test`; active. | `CORE-REQ-017` |
| crates/xmtp_id/src/scw_verifier/chain_rpc_verifier.rs | `scw_verifier::chain_rpc_verifier::tests::test_coinbase_smart_wallet` | Native-only async rstest fixture; 30-second timeout. | `CORE-REQ-018` |
| crates/xmtp_id/src/scw_verifier/chain_rpc_verifier.rs | `scw_verifier::chain_rpc_verifier::tests::test_smart_wallet_time_travel` | Native-only async rstest fixture; 60-second timeout; latest and historical blocks. | `CORE-REQ-018` |
| crates/xmtp_id/src/scw_verifier/chain_rpc_verifier.rs | `scw_verifier::chain_rpc_verifier::tests::test_is_valid_signature` | Native-only async rstest fixture; 60-second timeout; SCW and EOA cases. | `CORE-REQ-018` |
| crates/xmtp_id/src/associations/unsigned_actions.rs | `associations::unsigned_actions::tests::create_signatures` | Unit test selected by native or WASM `cfg_attr`; six action forms. | `CORE-REQ-021` |
| crates/xmtp_id/src/associations/builder.rs | `associations::builder::tests::create_inbox` | `xmtp_common::test`; async declaration. | `CORE-REQ-022` |
| crates/xmtp_id/src/associations/builder.rs | `associations::builder::tests::create_and_add_identity` | Async test selected by native or WASM `cfg_attr`. | `CORE-REQ-022` |
| crates/xmtp_id/src/associations/builder.rs | `associations::builder::tests::create_and_revoke` | Async test selected by native or WASM `cfg_attr`. | `CORE-REQ-022` |
| crates/xmtp_id/src/associations/builder.rs | `associations::builder::tests::attempt_adding_unknown_signer` | Async test selected by native or WASM `cfg_attr`. | `CORE-REQ-022` |
| crates/xmtp_id/src/associations/serialization.rs | `associations::serialization::tests::test_round_trip_unverified` | Unit test selected by native or WASM `cfg_attr`; four action variants. | `CORE-REQ-026` |
| crates/xmtp_id/src/associations/serialization.rs | `associations::serialization::tests::test_account_id` | Unit test selected by native or WASM `cfg_attr`; seven valid formats and one invalid format. | `CORE-REQ-027` |
| crates/xmtp_id/src/associations/serialization.rs | `associations::serialization::tests::test_account_id_create` | Unit test selected by native or WASM `cfg_attr`. | `CORE-REQ-027` |
| crates/xmtp_id/src/associations/unverified.rs | `associations::unverified::tests::create_identity_update` | Unit test selected by native or WASM `cfg_attr`. | `CORE-REQ-029` |
| crates/xmtp_id/src/associations/signature.rs | `associations::signature::tests::test_signature_error_verifier_retryable_propagates` | `xmtp_common::test`; active. | `CORE-REQ-030` |
| crates/xmtp_id/src/associations/signature.rs | `associations::signature::tests::test_signature_error_verifier_non_retryable_propagates` | `xmtp_common::test`; active. | `CORE-REQ-030` |
| crates/xmtp_id/src/associations/signature.rs | `associations::signature::tests::test_signature_error_non_verifier_variants_not_retryable` | `xmtp_common::test`; four error variants. | `CORE-REQ-030` |
| crates/xmtp_id/src/associations/signature.rs | `associations::signature::tests::test_to_lower_s` | `xmtp_common::test`; low-s and manipulated high-s cases. | `CORE-REQ-031` |
| crates/xmtp_id/src/associations/signature.rs | `associations::signature::tests::test_invalid_signature` | `wasm_bindgen_test` with native fallback; invalid scalar and wrong-length cases. | `CORE-REQ-031` |
| crates/xmtp_id/src/associations/member.rs | `associations::member::tests::test_identifier_comparisons` | Unit test selected by native or WASM `cfg_attr`; Ethereum and installation cases. | `CORE-REQ-033` |
| crates/xmtp_id/src/associations/verified_signature.rs | `associations::verified_signature::tests::test_recoverable_ecdsa` | Async test selected by native or WASM `cfg_attr`. | `CORE-REQ-034` |
| crates/xmtp_id/src/associations/verified_signature.rs | `associations::verified_signature::tests::test_recoverable_ecdsa_incorrect` | Async test selected by native or WASM `cfg_attr`; wrong-text recovery case. | `CORE-REQ-034` |
| crates/xmtp_id/src/associations/verified_signature.rs | `associations::verified_signature::tests::test_installation_key` | Async test selected by native or WASM `cfg_attr`; correct, wrong-text, and wrong-key cases. | `CORE-REQ-034` |
| crates/xmtp_id/src/associations/verified_signature.rs | `associations::verified_signature::tests::validate_good_key_round_trip` | Unit test selected by native or WASM `cfg_attr`; fixed legacy vector. | `CORE-REQ-036` |
| crates/xmtp_id/src/associations/verified_signature.rs | `associations::verified_signature::tests::validate_malformed_key` | Unit test selected by native or WASM `cfg_attr`; one-byte corruption. | `CORE-REQ-036` |
| crates/xmtp_id/src/associations/verified_signature.rs | `associations::verified_signature::tests::test_smart_contract_wallet` | Async test selected by native or WASM `cfg_attr`; mock verifier. | `CORE-REQ-034` |
| crates/xmtp_id/src/associations/state.rs | `associations::state::tests::can_add_remove` | Unit test selected by native or WASM `cfg_attr`. | `CORE-REQ-039` |
| crates/xmtp_id/src/associations/state.rs | `associations::state::tests::can_diff` | Unit test selected by native or WASM `cfg_attr`. | `CORE-REQ-039` |
| crates/xmtp_id/src/associations/mod.rs | `associations::tests::test_create_inbox` | `wasm_bindgen_test` with native fallback. | `CORE-REQ-040` |
| crates/xmtp_id/src/associations/mod.rs | `associations::tests::create_and_add_separately` | `wasm_bindgen_test` with native fallback. | `CORE-REQ-040` |
| crates/xmtp_id/src/associations/mod.rs | `associations::tests::create_and_add_together` | `wasm_bindgen_test` with native fallback. | `CORE-REQ-040` |
| crates/xmtp_id/src/associations/mod.rs | `associations::tests::create_from_legacy_key` | `wasm_bindgen_test` with native fallback; replay case. | `CORE-REQ-042` |
| crates/xmtp_id/src/associations/mod.rs | `associations::tests::add_wallet_from_installation_key` | `wasm_bindgen_test` with native fallback. | `CORE-REQ-043` |
| crates/xmtp_id/src/associations/mod.rs | `associations::tests::reject_invalid_signature_on_create` | `wasm_bindgen_test` with native fallback. | `CORE-REQ-044` |
| crates/xmtp_id/src/associations/mod.rs | `associations::tests::reject_invalid_signature_on_update` | `wasm_bindgen_test` with native fallback; existing- and new-member mismatch cases. | `CORE-REQ-044` |
| crates/xmtp_id/src/associations/mod.rs | `associations::tests::reject_if_signer_not_existing_member` | `wasm_bindgen_test` with native fallback. | `CORE-REQ-044` |
| crates/xmtp_id/src/associations/mod.rs | `associations::tests::reject_if_installation_adding_installation` | `wasm_bindgen_test` with native fallback. | `CORE-REQ-045` |
| crates/xmtp_id/src/associations/mod.rs | `associations::tests::revoke` | `wasm_bindgen_test` with native fallback. | `CORE-REQ-046` |
| crates/xmtp_id/src/associations/mod.rs | `associations::tests::revoke_children` | `wasm_bindgen_test` with native fallback. | `CORE-REQ-046` |
| crates/xmtp_id/src/associations/mod.rs | `associations::tests::revoke_and_re_add` | `wasm_bindgen_test` with native fallback. | `CORE-REQ-046` |
| crates/xmtp_id/src/associations/mod.rs | `associations::tests::change_recovery_address` | `wasm_bindgen_test` with native fallback. | `CORE-REQ-047` |
| crates/xmtp_id/src/associations/mod.rs | `associations::tests::scw_signature_binding` | `wasm_bindgen_test` with native fallback; loops over add, revoke, and recovery change. | `CORE-REQ-048` |
| crates/xmtp_db/src/encrypted_store/association_state.rs | `encrypted_store::association_state::tests::test_batch_read` | `xmtp_common::test`; one-key, two-key, and mismatch cases. | `CORE-REQ-049` |
| crates/xmtp_db/src/encrypted_store/consent_record.rs | `encrypted_store::consent_record::tests::find_consent_by_dm_id` | `xmtp_common::test`; `unwrap_try`. | `CORE-REQ-050` |
| crates/xmtp_db/src/encrypted_store/consent_record.rs | `encrypted_store::consent_record::tests::insert_and_read` | `xmtp_common::test`; insert, duplicate, replace, and conflict cases. | `CORE-REQ-051` |
| crates/xmtp_db/src/encrypted_store/conversation_list.rs | `encrypted_store::conversation_list::tests::test_single_group_multiple_messages` | `xmtp_common::test`; four messages. | `CORE-REQ-056` |
| crates/xmtp_db/src/encrypted_store/conversation_list.rs | `encrypted_store::conversation_list::tests::test_three_groups_specific_ordering` | `xmtp_common::test`; creation and activity ordering. | `CORE-REQ-056` |
| crates/xmtp_db/src/encrypted_store/conversation_list.rs | `encrypted_store::conversation_list::tests::test_group_with_newer_message_update` | `xmtp_common::test`; initial and later message. | `CORE-REQ-056` |
| crates/xmtp_db/src/encrypted_store/conversation_list.rs | `encrypted_store::conversation_list::tests::test_find_conversations_by_consent_state` | `xmtp_common::test`; all consent-state sets. | `CORE-REQ-056` |
| crates/xmtp_db/src/encrypted_store/conversation_list.rs | `encrypted_store::conversation_list::tests::test_find_conversations_default_excludes_denied` | `xmtp_common::test`; Allowed, Denied, and missing consent. | `CORE-REQ-056` |
| crates/xmtp_db/src/encrypted_store/conversation_list.rs | `encrypted_store::conversation_list::tests::test_unknown_content_type_is_present` | `xmtp_common::test`; `unwrap_try`. | `CORE-REQ-056` |
| crates/xmtp_db/src/encrypted_store/conversation_list.rs | `encrypted_store::conversation_list::tests::test_last_activity_after_ns_filter` | `xmtp_common::test`; thresholds 2500, 3500, and 4500. | `CORE-REQ-056` |
| crates/xmtp_db/src/encrypted_store/conversation_list.rs | `encrypted_store::conversation_list::tests::test_last_activity_before_ns_filter` | `xmtp_common::test`; thresholds 3500, 4500, and 5500. | `CORE-REQ-056` |
| crates/xmtp_db/src/encrypted_store/conversation_list.rs | `encrypted_store::conversation_list::tests::test_activity_filters_combined_with_limit` | `xmtp_common::test`; activity order and limit 2. | `CORE-REQ-056` |
| crates/xmtp_db/src/encrypted_store/d14n_migration_cutover.rs | `encrypted_store::d14n_migration_cutover::tests::test_default_migration_cutover` | `xmtp_common::test`; active. | `CORE-REQ-052` |
| crates/xmtp_db/src/encrypted_store/d14n_migration_cutover.rs | `encrypted_store::d14n_migration_cutover::tests::test_set_cutover_ns` | `xmtp_common::test`; active. | `CORE-REQ-052` |
| crates/xmtp_db/src/encrypted_store/d14n_migration_cutover.rs | `encrypted_store::d14n_migration_cutover::tests::test_set_last_checked_ns` | `xmtp_common::test`; active. | `CORE-REQ-052` |
| crates/xmtp_db/src/encrypted_store/d14n_migration_cutover.rs | `encrypted_store::d14n_migration_cutover::tests::test_get_last_checked_ns` | `xmtp_common::test`; active. | `CORE-REQ-052` |
| crates/xmtp_db/src/encrypted_store/d14n_migration_cutover.rs | `encrypted_store::d14n_migration_cutover::tests::test_set_has_migrated` | `xmtp_common::test`; active. | `CORE-REQ-052` |
| crates/xmtp_db/src/encrypted_store/database.rs | `encrypted_store::database::persistent_or_mem_tests::single_arm_dispatches` | `#[test]`; stub connection. | `CORE-REQ-054` |
| crates/xmtp_db/src/encrypted_store/database.rs | `encrypted_store::database::persistent_or_mem_tests::infallible_single_arm_compiles` | `#[test]`; WASM-shaped `Infallible` Single type. | `CORE-REQ-054` |
| crates/xmtp_db/src/encrypted_store/database/instrumentation.rs | `encrypted_store::database::instrumentation::tests::db_lock_panic_enabled_by_default` | `#[test]`; missing environment value. | `CORE-REQ-055` |
| crates/xmtp_db/src/encrypted_store/database/instrumentation.rs | `encrypted_store::database::instrumentation::tests::db_lock_panic_opt_out_values` | `#[test]`; values `1` and `true`. | `CORE-REQ-055` |
| crates/xmtp_db/src/encrypted_store/database/instrumentation.rs | `encrypted_store::database::instrumentation::tests::db_lock_panic_ignores_other_values` | `#[test]`; six other values. | `CORE-REQ-055` |
| crates/xmtp_db/src/encrypted_store/database/native.rs | `encrypted_store::database::native::tests::releases_db_lock` | Native-only `tokio::test`; persistent pool release and reconnect. | `CORE-REQ-063` |
| crates/xmtp_db/src/encrypted_store/database/native.rs | `encrypted_store::database::native::tests::mismatched_encryption_key` | Native-only `tokio::test`; pooled wrong-key case. | `CORE-REQ-064` |
| crates/xmtp_db/src/encrypted_store/database/native.rs | `encrypted_store::database::native::tests::single_connection_roundtrip_and_reconnect` | Native-only `tokio::test`; Single arm. | `CORE-REQ-063` |
| crates/xmtp_db/src/encrypted_store/database/native.rs | `encrypted_store::database::native::tests::single_connection_disconnect_releases_then_reconnect` | Native-only `tokio::test`; disconnected query and reconnect. | `CORE-REQ-063` |
| crates/xmtp_db/src/encrypted_store/database/native.rs | `encrypted_store::database::native::tests::single_connection_mismatched_key_fails` | Native-only `tokio::test`; Single wrong-key case. | `CORE-REQ-064` |
| crates/xmtp_db/src/encrypted_store/database/native.rs | `encrypted_store::database::native::tests::single_connection_nested_transaction_no_deadlock` | Native-only `tokio::test`; transaction plus nested savepoint. | `CORE-REQ-067` |
| crates/xmtp_db/src/encrypted_store/database/native/pool.rs | `encrypted_store::database::native::pool::tests::sets_busy_timeout` | Ignored rstest declaration; encrypted and unencrypted cases. | `CORE-REQ-062` |
| crates/xmtp_db/src/encrypted_store/database/native/pool.rs | `encrypted_store::database::native::pool::tests::sets_journal_mode` | Ignored rstest declaration; encrypted and unencrypted cases. | `CORE-REQ-062` |
| crates/xmtp_db/src/encrypted_store/database/native/pool.rs | `encrypted_store::database::native::pool::tests::sets_synchronous` | Ignored rstest declaration; encrypted and unencrypted cases. | `CORE-REQ-062` |
| crates/xmtp_db/src/encrypted_store/database/native/pool.rs | `encrypted_store::database::native::pool::tests::sets_autocheckpoint` | Ignored rstest declaration; encrypted and unencrypted cases. | `CORE-REQ-062` |
| crates/xmtp_db/src/encrypted_store/database/native/sqlcipher_connection.rs | `encrypted_store::database::native::sqlcipher_connection::tests::test_sqlcipher_version` | Native-only `tokio::test`. | `CORE-REQ-068` |
| crates/xmtp_db/src/encrypted_store/database/native/sqlcipher_connection.rs | `encrypted_store::database::native::sqlcipher_connection::tests::test_db_creates_with_plaintext_header` | Native-only `tokio::test`; new database. | `CORE-REQ-069` |
| crates/xmtp_db/src/encrypted_store/database/native/sqlcipher_connection.rs | `encrypted_store::database::native::sqlcipher_connection::tests::test_db_migrates` | Native-only `tokio::test`; legacy encrypted-header database. | `CORE-REQ-070` |
| crates/xmtp_db/src/encrypted_store/group.rs | `encrypted_store::group::tests::test_it_stores_group` | `xmtp_common::test`; active. | `CORE-REQ-071` |
| crates/xmtp_db/src/encrypted_store/group.rs | `encrypted_store::group::tests::test_it_fetches_group` | `xmtp_common::test`; active. | `CORE-REQ-071` |
| crates/xmtp_db/src/encrypted_store/group.rs | `encrypted_store::group::tests::test_it_updates_group_membership_state` | `xmtp_common::test`; Pending to Rejected. | `CORE-REQ-071` |
| crates/xmtp_db/src/encrypted_store/group.rs | `encrypted_store::group::tests::test_find_groups` | `xmtp_common::test`; async declaration; WASM clock wait; state, type, limit, time, and DM cases. | `CORE-REQ-073` |
| crates/xmtp_db/src/encrypted_store/group.rs | `encrypted_store::group::tests::test_installations_last_checked_is_updated` | `xmtp_common::test`; async declaration; WASM clock wait. | `CORE-REQ-071` |
| crates/xmtp_db/src/encrypted_store/group.rs | `encrypted_store::group::tests::test_new_group_has_correct_purpose` | `xmtp_common::test`; active. | `CORE-REQ-071` |
| crates/xmtp_db/src/encrypted_store/group.rs | `encrypted_store::group::tests::test_find_groups_by_consent_state` | `xmtp_common::test`; all consent-state sets. | `CORE-REQ-073` |
| crates/xmtp_db/src/encrypted_store/group.rs | `encrypted_store::group::tests::test_get_sequence_ids` | `xmtp_common::test`; cursor-bearing and cursorless groups. | `CORE-REQ-077` |
| crates/xmtp_db/src/encrypted_store/group.rs | `encrypted_store::group::tests::test_insert_or_replace_group_update_preserves_originator` | `xmtp_common::test`; regression case. | `CORE-REQ-077` |
| crates/xmtp_db/src/encrypted_store/group.rs | `encrypted_store::group::tests::test_group_cursors_skips_row_with_null_originator` | `xmtp_common::test`; raw malformed-row regression. | `CORE-REQ-077` |
| crates/xmtp_db/src/encrypted_store/group.rs | `encrypted_store::group::tests::test_find_group_default_excludes_denied` | `xmtp_common::test`; Allowed, Denied, and missing consent. | `CORE-REQ-073` |
| crates/xmtp_db/src/encrypted_store/group.rs | `encrypted_store::group::tests::test_get_conversation_ids_for_remote_log_publish` | `xmtp_common::test`; `unwrap_try`; publish flag, consent, and key cases. | `CORE-REQ-078` |
| crates/xmtp_db/src/encrypted_store/group.rs | `encrypted_store::group::tests::test_get_conversation_ids_for_remote_log_publish_with_consent` | `xmtp_common::test`; Allowed, Denied, and missing consent. | `CORE-REQ-078` |
| crates/xmtp_db/src/encrypted_store/group.rs | `encrypted_store::group::tests::test_get_conversation_ids_for_remote_log_download_with_consent` | `xmtp_common::test`; consent and Sync exclusion. | `CORE-REQ-079` |
| crates/xmtp_db/src/encrypted_store/group.rs | `encrypted_store::group::tests::test_get_conversation_ids_for_responding_readds` | `xmtp_common::test`; five re-add status shapes. | `CORE-REQ-080` |
| crates/xmtp_db/src/encrypted_store/group.rs | `encrypted_store::group::tests::test_find_group_span_emits_operation_and_skips_group_id` | `#[test]`; custom capture subscriber. | `CORE-REQ-081` |
| crates/xmtp_db/src/encrypted_store/group/dms.rs | `encrypted_store::group::dms::tests::test_dm_stitching` | `xmtp_common::test`; two rows with one DM ID. | `CORE-REQ-073` |
| crates/xmtp_db/src/encrypted_store/group/dms.rs | `encrypted_store::group::dms::tests::test_dm_deduplication` | `xmtp_common::test`; three duplicate DMs, unrelated DM and group, and duplicate toggle. | `CORE-REQ-073` |
| crates/xmtp_db/src/encrypted_store/group_intent.rs | `encrypted_store::group_intent::tests::intent_kind_discriminants_are_contiguous` | `xmtp_common::test`; all variants. | `CORE-REQ-083` |
| crates/xmtp_db/src/encrypted_store/group_intent.rs | `encrypted_store::group_intent::tests::unknown_kind_row_is_excluded_by_kind_filter` | `xmtp_common::test`; raw future-kind row and filtered or unfiltered queries. | `CORE-REQ-083` |
| crates/xmtp_db/src/encrypted_store/group_intent.rs | `encrypted_store::group_intent::tests::test_store_and_fetch` | `xmtp_common::test`; active. | `CORE-REQ-085` |
| crates/xmtp_db/src/encrypted_store/group_intent.rs | `encrypted_store::group_intent::tests::test_query` | `xmtp_common::test`; state and kind combinations. | `CORE-REQ-085` |
| crates/xmtp_db/src/encrypted_store/group_intent.rs | `encrypted_store::group_intent::tests::find_by_payload_hash` | `xmtp_common::test`; published hash lookup. | `CORE-REQ-086` |
| crates/xmtp_db/src/encrypted_store/group_intent.rs | `encrypted_store::group_intent::tests::test_happy_path_state_transitions` | `xmtp_common::test`; ToPublish, Published, and Committed. | `CORE-REQ-087` |
| crates/xmtp_db/src/encrypted_store/group_intent.rs | `encrypted_store::group_intent::tests::test_republish_state_transition` | `xmtp_common::test`; Published back to ToPublish. | `CORE-REQ-087` |
| crates/xmtp_db/src/encrypted_store/group_intent.rs | `encrypted_store::group_intent::tests::test_invalid_state_transition` | `xmtp_common::test`; two invalid transitions. | `CORE-REQ-087` |
| crates/xmtp_db/src/encrypted_store/group_intent.rs | `encrypted_store::group_intent::tests::test_increment_publish_attempts` | `xmtp_common::test`; two increments. | `CORE-REQ-090` |
| crates/xmtp_db/src/encrypted_store/group_intent.rs | `encrypted_store::group_intent::tests::test_find_dependant_commits` | `xmtp_common::test`; two payload hashes. | `CORE-REQ-091` |
| crates/xmtp_db/src/encrypted_store/group_intent.rs | `encrypted_store::group_intent::tests::bootstrap_migration_intent_round_trips_through_sql` | `xmtp_common::test`; SQL and display mapping. | `CORE-REQ-083` |
| crates/xmtp_db/src/encrypted_store/group_message/messages_newer_than_tests.rs | `encrypted_store::group_message::messages_newer_than_tests::test_messages_newer_than_basic` | `xmtp_common::test`; two originators. | `CORE-REQ-093` |
| crates/xmtp_db/src/encrypted_store/group_message/messages_newer_than_tests.rs | `encrypted_store::group_message::messages_newer_than_tests::test_messages_newer_than_new_originator` | `xmtp_common::test`; unseen originator. | `CORE-REQ-093` |
| crates/xmtp_db/src/encrypted_store/group_message/messages_newer_than_tests.rs | `encrypted_store::group_message::messages_newer_than_tests::test_messages_newer_than_multiple_groups` | `xmtp_common::test`; two groups. | `CORE-REQ-093` |
| crates/xmtp_db/src/encrypted_store/group_message/messages_newer_than_tests.rs | `encrypted_store::group_message::messages_newer_than_tests::test_messages_newer_than_batching` | `xmtp_common::test`; loop creates 150 groups. | `CORE-REQ-093` |
| crates/xmtp_db/src/encrypted_store/group_message/messages_newer_than_tests.rs | `encrypted_store::group_message::messages_newer_than_tests::test_messages_newer_than_empty_cursor` | `xmtp_common::test`; empty global cursor. | `CORE-REQ-093` |
| crates/xmtp_db/src/encrypted_store/group_message/messages_newer_than_tests.rs | `encrypted_store::group_message::messages_newer_than_tests::test_messages_newer_than_no_new_messages` | `xmtp_common::test`; cursors at current values. | `CORE-REQ-093` |
| crates/xmtp_db/src/encrypted_store/group_message/messages_newer_than_tests.rs | `encrypted_store::group_message::messages_newer_than_tests::test_messages_newer_than_mixed_originators` | `xmtp_common::test`; known and unseen originators. | `CORE-REQ-093` |
| crates/xmtp_db/src/encrypted_store/group_message/messages_newer_than_tests.rs | `encrypted_store::group_message::messages_newer_than_tests::test_messages_newer_than_empty_groups` | `xmtp_common::test`; group has no messages. | `CORE-REQ-093` |
| crates/xmtp_db/src/encrypted_store/group_message/messages_newer_than_tests.rs | `encrypted_store::group_message::messages_newer_than_tests::test_messages_newer_than_per_group_cursors` | `xmtp_common::test`; same originator with distinct group cursors. | `CORE-REQ-093` |
| crates/xmtp_db/src/encrypted_store/group_message/tests.rs | `encrypted_store::group_message::tests::it_does_not_error_on_empty_messages` | `xmtp_common::test`; missing ID. | `CORE-REQ-095` |
| crates/xmtp_db/src/encrypted_store/group_message/tests.rs | `encrypted_store::group_message::tests::test_exclude_content_types_filter` | `xmtp_common::test`; exclude Reaction and ReadReceipt in query and count. | `SHARED-CONTENT-REQ-007` |
| crates/xmtp_db/src/encrypted_store/group_message/tests.rs | `encrypted_store::group_message::tests::it_gets_messages` | `xmtp_common::test`; stored-row lookup. | `CORE-REQ-095` |
| crates/xmtp_db/src/encrypted_store/group_message/tests.rs | `encrypted_store::group_message::tests::it_cannot_insert_message_without_group` | `xmtp_common::test`; foreign-key failure. | `CORE-REQ-095` |
| crates/xmtp_db/src/encrypted_store/group_message/tests.rs | `encrypted_store::group_message::tests::it_gets_many_messages` | `xmtp_common::test`; loop creates 50 rows. | `CORE-REQ-097` |
| crates/xmtp_db/src/encrypted_store/group_message/tests.rs | `encrypted_store::group_message::tests::it_gets_messages_by_time` | `xmtp_common::test`; before, after, and combined strict ranges. | `SHARED-CONTENT-REQ-007` |
| crates/xmtp_db/src/encrypted_store/group_message/tests.rs | `encrypted_store::group_message::tests::it_deletes_middle_message_by_expiration_time` | `xmtp_common::test`; expired middle row. | `CORE-REQ-099` |
| crates/xmtp_db/src/encrypted_store/group_message/tests.rs | `encrypted_store::group_message::tests::it_gets_messages_by_kind` | `xmtp_common::test`; loop creates 15 Application and 15 MembershipChange rows. | `SHARED-CONTENT-REQ-007` |
| crates/xmtp_db/src/encrypted_store/group_message/tests.rs | `encrypted_store::group_message::tests::it_orders_messages_by_sent` | `xmtp_common::test`; ascending, descending, and group last-message update. | `CORE-REQ-097` |
| crates/xmtp_db/src/encrypted_store/group_message/tests.rs | `encrypted_store::group_message::tests::it_gets_messages_by_content_type` | `xmtp_common::test`; Text, GroupMembershipChange, and GroupUpdated. | `SHARED-CONTENT-REQ-007` |
| crates/xmtp_db/src/encrypted_store/group_message/tests.rs | `encrypted_store::group_message::tests::it_dedupes_group_updated_messages_from_dm_by_default` | `xmtp_common::test`; default and explicit GroupUpdated DM queries. | `SHARED-CONTENT-REQ-007` |
| crates/xmtp_db/src/encrypted_store/group_message/tests.rs | `encrypted_store::group_message::tests::test_inbound_relations_with_results` | `xmtp_common::test`; two targets with three reactions. | `CORE-REQ-101` |
| crates/xmtp_db/src/encrypted_store/group_message/tests.rs | `encrypted_store::group_message::tests::test_relations_when_no_references_exist` | `xmtp_common::test`; empty inbound and outbound results. | `CORE-REQ-101` |
| crates/xmtp_db/src/encrypted_store/group_message/tests.rs | `encrypted_store::group_message::tests::test_inbound_relations_no_main_query_results` | `xmtp_common::test`; empty ID input. | `CORE-REQ-101` |
| crates/xmtp_db/src/encrypted_store/group_message/tests.rs | `encrypted_store::group_message::tests::test_inbound_relations_with_limit` | `xmtp_common::test`; ten reactions and limit 3. | `CORE-REQ-101` |
| crates/xmtp_db/src/encrypted_store/group_message/tests.rs | `encrypted_store::group_message::tests::test_relations_with_content_type_filters` | `xmtp_common::test`; Reaction, Reply, ReadReceipt, Text, and Attachment relations. | `CORE-REQ-101` |
| crates/xmtp_db/src/encrypted_store/group_message/tests.rs | `encrypted_store::group_message::tests::test_outbound_relations_with_results` | `xmtp_common::test`; two original messages and replies. | `CORE-REQ-101` |
| crates/xmtp_db/src/encrypted_store/group_message/tests.rs | `encrypted_store::group_message::tests::test_outbound_relations_no_main_query_results` | `xmtp_common::test`; time filter produces empty IDs. | `CORE-REQ-101` |
| crates/xmtp_db/src/encrypted_store/group_message/tests.rs | `encrypted_store::group_message::tests::test_outbound_relations_with_limit` | `xmtp_common::test`; five references, caller takes two. | `CORE-REQ-101` |
| crates/xmtp_db/src/encrypted_store/group_message/tests.rs | `encrypted_store::group_message::tests::test_both_inbound_and_outbound_relations` | `xmtp_common::test`; original, reply, and two reactions. | `CORE-REQ-101` |
| crates/xmtp_db/src/encrypted_store/group_message/tests.rs | `encrypted_store::group_message::tests::test_relation_filters_none_behavior` | `xmtp_common::test`; direct message query plus separate inbound and outbound lookups. | `CORE-REQ-101` |
| crates/xmtp_db/src/encrypted_store/group_message/tests.rs | `encrypted_store::group_message::tests::test_complex_relation_chain` | `xmtp_common::test`; reply and reactions on original and reply. | `CORE-REQ-101` |
| crates/xmtp_db/src/encrypted_store/group_message/tests.rs | `encrypted_store::group_message::tests::test_inbound_relation_counts` | `xmtp_common::test`; all, Reaction, and Reply count filters. | `CORE-REQ-104` |
| crates/xmtp_db/src/encrypted_store/group_message/tests.rs | `encrypted_store::group_message::tests::test_get_latest_message_times_by_sender_single_sender` | `xmtp_common::test`; three times for one sender. | `CORE-REQ-105` |
| crates/xmtp_db/src/encrypted_store/group_message/tests.rs | `encrypted_store::group_message::tests::test_get_latest_message_times_by_sender_multiple_senders` | `xmtp_common::test`; three senders. | `CORE-REQ-105` |
| crates/xmtp_db/src/encrypted_store/group_message/tests.rs | `encrypted_store::group_message::tests::test_get_latest_message_times_by_sender_empty_results` | `xmtp_common::test`; empty and nonmatching content. | `CORE-REQ-105` |
| crates/xmtp_db/src/encrypted_store/group_message/tests.rs | `encrypted_store::group_message::tests::test_get_latest_message_times_by_sender_dm_group` | `xmtp_common::test`; three groups with one DM ID. | `CORE-REQ-105` |
| crates/xmtp_db/src/encrypted_store/group_message/tests.rs | `encrypted_store::group_message::tests::test_count_group_messages` | `xmtp_common::test`; content, kind, time, and delivery-status matrix. | `SHARED-CONTENT-REQ-007` |
| crates/xmtp_db/src/encrypted_store/group_message/tests.rs | `encrypted_store::group_message::tests::test_count_group_messages_dm_vs_regular_groups` | `xmtp_common::test`; identical DM and regular-group sets. | `SHARED-CONTENT-REQ-007` |
| crates/xmtp_db/src/encrypted_store/group_message/tests.rs | `encrypted_store::group_message::tests::test_count_group_messages_empty_groups` | `xmtp_common::test`; default and filtered empty counts. | `SHARED-CONTENT-REQ-007` |
| crates/xmtp_db/src/encrypted_store/group_message/tests.rs | `encrypted_store::group_message::tests::test_get_latest_message_times_by_sender_mixed_content_types` | `xmtp_common::test`; Text, Attachment, and combined cases. | `CORE-REQ-105` |
| crates/xmtp_db/src/encrypted_store/group_message/tests.rs | `encrypted_store::group_message::tests::it_deletes_message_by_id` | `xmtp_common::test`; first and repeated delete. | `CORE-REQ-095` |
| crates/xmtp_db/src/encrypted_store/group_message/tests.rs | `encrypted_store::group_message::tests::test_exclude_sender_inbox_ids_filter` | `xmtp_common::test`; one, multiple, all, absent, and combined sender exclusions. | `SHARED-CONTENT-REQ-007` |
| crates/xmtp_db/src/encrypted_store/group_message/tests.rs | `encrypted_store::group_message::tests::test_sort_by_sent_at` | `xmtp_common::test`; ascending and descending. | `CORE-REQ-097` |
| crates/xmtp_db/src/encrypted_store/group_message/tests.rs | `encrypted_store::group_message::tests::test_sort_by_inserted_at` | Native-only `xmtp_common::test`; sequential inserts with delays. | `CORE-REQ-108` |
| crates/xmtp_db/src/encrypted_store/group_message/tests.rs | `encrypted_store::group_message::tests::test_inserted_after_filter` | Native-only `xmtp_common::test`; strict after bound. | `CORE-REQ-108` |
| crates/xmtp_db/src/encrypted_store/group_message/tests.rs | `encrypted_store::group_message::tests::test_inserted_before_filter` | Native-only `xmtp_common::test`; strict before bound. | `CORE-REQ-108` |
| crates/xmtp_db/src/encrypted_store/group_message/tests.rs | `encrypted_store::group_message::tests::test_inserted_at_based_pagination` | Native-only `xmtp_common::test`; ten rows and three pages of three. | `CORE-REQ-108` |
| crates/xmtp_db/src/encrypted_store/group_message/tests.rs | `encrypted_store::group_message::tests::test_inserted_at_populated_in_all_queries` | `xmtp_common::test`; ID, timestamp, and paged query forms. | `CORE-REQ-108` |
| crates/xmtp_db/src/encrypted_store/group_message/tests.rs | `encrypted_store::group_message::tests::test_expired_messages_excluded_from_queries` | `xmtp_common::test`; null, past, and future expiry. | `CORE-REQ-099` |
| crates/xmtp_db/src/encrypted_store/group_message/tests.rs | `encrypted_store::group_message::tests::test_content_type_is_deletable` | `#[test]`; user, system, metadata, delete, and unknown content matrix. | `SHARED-CONTENT-REQ-005` |
| crates/xmtp_db/src/encrypted_store/group_message/tests.rs | `encrypted_store::group_message::tests::test_group_message_kind_is_deletable` | `#[test]`; Application and MembershipChange. | `SHARED-CONTENT-REQ-005` |
| crates/xmtp_db/src/encrypted_store/group_message/tests.rs | `encrypted_store::group_message::tests::test_min_expire_at_ns` | `xmtp_common::test`; `unwrap_try`; empty, null, and two expiries. | `CORE-REQ-099` |
| crates/xmtp_db/src/encrypted_store/icebox.rs | `encrypted_store::icebox::tests::icebox_dependency_chain` | `xmtp_common::test`; `unwrap_try`; past and future traversal. | `CORE-REQ-110` |
| crates/xmtp_db/src/encrypted_store/icebox.rs | `encrypted_store::icebox::tests::test_icebox_wrong_originator` | `xmtp_common::test`; `unwrap_try`; broken originator link. | `CORE-REQ-110` |
| crates/xmtp_db/src/encrypted_store/icebox.rs | `encrypted_store::icebox::tests::test_icebox_wrong_sequence` | `xmtp_common::test`; `unwrap_try`; broken sequence link. | `CORE-REQ-110` |
| crates/xmtp_db/src/encrypted_store/icebox.rs | `encrypted_store::icebox::tests::test_icebox_multiple_dependencies` | `xmtp_common::test`; `unwrap_try`; fan-out of two dependents. | `CORE-REQ-110` |
| crates/xmtp_db/src/encrypted_store/icebox.rs | `encrypted_store::icebox::tests::test_icebox_chain` | `xmtp_common::test`; `unwrap_try`; chained commit and application envelopes. | `CORE-REQ-110` |
| crates/xmtp_db/src/encrypted_store/icebox.rs | `encrypted_store::icebox::tests::test_future_dependents_multiple_cursors` | `xmtp_common::test`; `unwrap_try`; two starting cursors and deduplication. | `CORE-REQ-110` |
| crates/xmtp_db/src/encrypted_store/icebox.rs | `encrypted_store::icebox::tests::test_future_dependents_empty` | `xmtp_common::test`; `unwrap_try`; empty input. | `CORE-REQ-110` |
| crates/xmtp_db/src/encrypted_store/icebox.rs | `encrypted_store::icebox::tests::test_querying_dependencies_in_middle_works` | `xmtp_common::test`; `unwrap_try`; middle cursor in both directions. | `CORE-REQ-110` |
| crates/xmtp_db/src/encrypted_store/icebox.rs | `encrypted_store::icebox::tests::test_prune_icebox` | `xmtp_common::test`; `unwrap_try`; same-originator threshold and another originator. | `CORE-REQ-111` |
| crates/xmtp_db/src/encrypted_store/icebox.rs | `encrypted_store::icebox::tests::test_prune_icebox_no_cleanup_when_cursor_lower` | `xmtp_common::test`; `unwrap_try`; lower refresh cursor. | `CORE-REQ-111` |
| crates/xmtp_db/src/encrypted_store/icebox.rs | `encrypted_store::icebox::tests::test_prune_icebox_only_relevant_entity_kinds` | `xmtp_common::test`; `unwrap_try`; Welcome does not prune. | `CORE-REQ-111` |
| crates/xmtp_db/src/encrypted_store/icebox.rs | `encrypted_store::icebox::tests::test_prune_icebox_dependencies_cascade_deleted` | `xmtp_common::test`; `unwrap_try`; equal cursor and cascade. | `CORE-REQ-111` |
| crates/xmtp_db/src/encrypted_store/identity.rs | `encrypted_store::identity::tests::queue_with_nudge_is_noop_before_registration` | `xmtp_common::test`; empty identity table. | `CORE-REQ-112` |
| crates/xmtp_db/src/encrypted_store/identity.rs | `encrypted_store::identity::tests::queue_with_nudge_selfheals_missing_seed` | `xmtp_common::test`; identity exists and seed is absent. | `CORE-REQ-112` |
| crates/xmtp_db/src/encrypted_store/identity.rs | `encrypted_store::identity::tests::queue_initializes_null_rotation_column` | `xmtp_common::test`; null column and repeated queue. | `CORE-REQ-112` |
| crates/xmtp_db/src/encrypted_store/identity.rs | `encrypted_store::identity::tests::can_only_store_one_identity` | `xmtp_common::test`; async declaration; duplicate insert. | `CORE-REQ-115` |
| crates/xmtp_db/src/encrypted_store/identity_cache.rs | `encrypted_store::identity_cache::tests::test_store_duplicated_wallets` | `xmtp_common::test`; duplicate Ethereum identity. | `CORE-REQ-116` |
| crates/xmtp_db/src/encrypted_store/identity_cache.rs | `encrypted_store::identity_cache::tests::test_fetch_and_store_identity_cache` | `xmtp_common::test`; cached, uncached, and missing lists. | `CORE-REQ-116` |
| crates/xmtp_db/src/encrypted_store/identity_update.rs | `encrypted_store::identity_update::tests::insert_and_read` | `xmtp_common::test`; two ordered updates. | `CORE-REQ-117` |
| crates/xmtp_db/src/encrypted_store/identity_update.rs | `encrypted_store::identity_update::tests::test_filter` | `xmtp_common::test`; three range shapes. | `CORE-REQ-117` |
| crates/xmtp_db/src/encrypted_store/identity_update.rs | `encrypted_store::identity_update::tests::test_get_latest_sequence_id` | `xmtp_common::test`; two inboxes and missing inbox. | `CORE-REQ-117` |
| crates/xmtp_db/src/encrypted_store/identity_update.rs | `encrypted_store::identity_update::tests::get_single_sequence_id` | `xmtp_common::test`; one inbox with two updates. | `CORE-REQ-117` |
| crates/xmtp_db/src/encrypted_store/identity_update.rs | `encrypted_store::identity_update::tests::test_count_inbox_updates` | `xmtp_common::test`; two existing and one missing inbox. | `CORE-REQ-117` |
| crates/xmtp_db/src/encrypted_store/key_package_history.rs | `encrypted_store::key_package_history::tests::min_key_package_delete_at_ns_none_when_empty` | `xmtp_common::test`; empty table. | `CORE-REQ-120` |
| crates/xmtp_db/src/encrypted_store/key_package_history.rs | `encrypted_store::key_package_history::tests::test_store_key_package_history_entry` | `xmtp_common::test`; store with PQ key and exact delete. | `CORE-REQ-120` |
| crates/xmtp_db/src/encrypted_store/key_package_history.rs | `encrypted_store::key_package_history::tests::test_store_multiple` | `xmtp_common::test`; three entries and strict before-ID query. | `CORE-REQ-120` |
| crates/xmtp_db/src/encrypted_store/message_deletion.rs | `encrypted_store::message_deletion::tests::test_store_and_get_deletion` | `xmtp_common::test`; `unwrap_try`; lookup by deletion and target IDs. | `SHARED-CONTENT-REQ-006` |
| crates/xmtp_db/src/encrypted_store/message_deletion.rs | `encrypted_store::message_deletion::tests::test_is_message_deleted` | `xmtp_common::test`; `unwrap_try`; before and after deletion. | `SHARED-CONTENT-REQ-006` |
| crates/xmtp_db/src/encrypted_store/message_deletion.rs | `encrypted_store::message_deletion::tests::test_get_deletions_for_messages` | `xmtp_common::test`; `unwrap_try`; two deleted and one undeleted targets. | `SHARED-CONTENT-REQ-006` |
| crates/xmtp_db/src/encrypted_store/message_deletion.rs | `encrypted_store::message_deletion::tests::test_get_group_deletions` | `xmtp_common::test`; `unwrap_try`; two-group isolation. | `SHARED-CONTENT-REQ-006` |
| crates/xmtp_db/src/encrypted_store/migration_test/add_inserted_at_ns.rs | `encrypted_store::migration_test::add_inserted_at_ns::migration_performance_10k_messages` | Native-only `xmtp_common::test`; async; 10000 rows all checked; host-sensitive one-second wall-clock gate. | `CORE-REQ-123` |
| crates/xmtp_db/src/encrypted_store/migration_test/originator_id_refresh_state.rs | `encrypted_store::migration_test::originator_id_refresh_state::up_groups` | `xmtp_common::test`; async declaration; group-originator up migration. | `CORE-REQ-124` |
| crates/xmtp_db/src/encrypted_store/migration_test/originator_id_refresh_state.rs | `encrypted_store::migration_test::originator_id_refresh_state::up_identity_updates` | `xmtp_common::test`; async declaration; identity-update up migration. | `CORE-REQ-125` |
| crates/xmtp_db/src/encrypted_store/migration_test/originator_id_refresh_state.rs | `encrypted_store::migration_test::originator_id_refresh_state::down_identity_updates` | `xmtp_common::test`; async declaration; identity-update down migration. | `CORE-REQ-125` |
| crates/xmtp_db/src/encrypted_store/migration_test/originator_id_refresh_state.rs | `encrypted_store::migration_test::originator_id_refresh_state::up_both_cursors_set_to_old_value` | `xmtp_common::test`; async declaration; commit and application cursor split. | `CORE-REQ-126` |
| crates/xmtp_db/src/encrypted_store/migration_test/originator_id_refresh_state.rs | `encrypted_store::migration_test::originator_id_refresh_state::up_welcome_unchanged` | `xmtp_common::test`; async declaration; Welcome cursor. | `CORE-REQ-126` |
| crates/xmtp_db/src/encrypted_store/migration_test/originator_id_refresh_state.rs | `encrypted_store::migration_test::originator_id_refresh_state::down` | `xmtp_common::test`; async declaration; merge by maximum and retain Welcome. | `CORE-REQ-127` |
| crates/xmtp_db/src/encrypted_store/migration_test/update_dm_trigger.rs | `encrypted_store::migration_test::update_dm_trigger::update_dm_trigger` | `xmtp_common::test`; async declaration; completion without an explicit assertion. | `CORE-REQ-128` |
| crates/xmtp_db/src/encrypted_store/mod.rs | `encrypted_store::tests::ephemeral_store` | `xmtp_common::test`; async declaration. | `CORE-REQ-129` |
| crates/xmtp_db/src/encrypted_store/mod.rs | `encrypted_store::tests::persistent_store` | `xmtp_common::test`; async declaration. | `CORE-REQ-129` |
| crates/xmtp_db/src/encrypted_store/mod.rs | `encrypted_store::tests::encrypted_db_with_multiple_connections` | `xmtp_common::test`; async declaration; two handles. | `CORE-REQ-129` |
| crates/xmtp_db/src/encrypted_store/mod.rs | `encrypted_store::tests::pool_failure_needs_connection` | Native-only `xmtp_common::test`; async declaration; real disconnected pool. | `CORE-REQ-130` |
| crates/xmtp_db/src/encrypted_store/mod.rs | `encrypted_store::db_needs_connection_tests::pool_errors_need_connection` | Native-only `#[test]`; direct, wrapped, and unrelated errors. | `CORE-REQ-130` |
| crates/xmtp_db/src/encrypted_store/pending_remove.rs | `encrypted_store::pending_remove::tests::test_add_pending_remove` | `xmtp_common::test`; `unwrap_try`; target and other group. | `CORE-REQ-131` |
| crates/xmtp_db/src/encrypted_store/pending_remove.rs | `encrypted_store::pending_remove::tests::test_delete_pending_remove_user` | `xmtp_common::test`; `unwrap_try`; delete two of three and other-group no-op. | `CORE-REQ-131` |
| crates/xmtp_db/src/encrypted_store/processed_device_sync_messages.rs | `encrypted_store::processed_device_sync_messages::tests::it_marks_as_processed` | `xmtp_common::test`; `unwrap_try`; untracked, Pending, and Processed states. | `CORE-REQ-132` |
| crates/xmtp_db/src/encrypted_store/processed_device_sync_messages.rs | `encrypted_store::processed_device_sync_messages::tests::it_stores_with_attempts_and_state` | `xmtp_common::test`; `unwrap_try`; zero-attempt Pending default. | `CORE-REQ-132` |
| crates/xmtp_db/src/encrypted_store/processed_device_sync_messages.rs | `encrypted_store::processed_device_sync_messages::tests::it_preserves_attempts_when_marking_as_processed` | `xmtp_common::test`; `unwrap_try`; two attempts. | `CORE-REQ-132` |
| crates/xmtp_db/src/encrypted_store/processed_device_sync_messages.rs | `encrypted_store::processed_device_sync_messages::tests::it_increments_attempts_and_sets_failed_at_max` | `xmtp_common::test`; `unwrap_try`; attempts one through three. | `CORE-REQ-132` |
| crates/xmtp_db/src/encrypted_store/processed_device_sync_messages.rs | `encrypted_store::processed_device_sync_messages::tests::it_returns_sync_group_messages_paged` | `xmtp_common::test`; `unwrap_try`; five Sync rows, one DM row, and four page offsets. | `CORE-REQ-132` |
| crates/xmtp_db/src/encrypted_store/readd_status.rs | `encrypted_store::readd_status::tests::test_get_readd_status_not_found` | `xmtp_common::test`; missing composite key. | `CORE-REQ-135` |
| crates/xmtp_db/src/encrypted_store/readd_status.rs | `encrypted_store::readd_status::tests::test_store_and_get_readd_status` | `xmtp_common::test`; request and response round-trip. | `CORE-REQ-135` |
| crates/xmtp_db/src/encrypted_store/readd_status.rs | `encrypted_store::readd_status::tests::test_update_requested_at_sequence_id_creates_new` | `xmtp_common::test`; absent row. | `CORE-REQ-135` |
| crates/xmtp_db/src/encrypted_store/readd_status.rs | `encrypted_store::readd_status::tests::test_update_requested_at_sequence_id_updates_existing` | `xmtp_common::test`; higher request and preserved response. | `CORE-REQ-135` |
| crates/xmtp_db/src/encrypted_store/readd_status.rs | `encrypted_store::readd_status::tests::test_update_requested_at_sequence_id_only_updates_if_higher` | `xmtp_common::test`; lower request ignored. | `CORE-REQ-135` |
| crates/xmtp_db/src/encrypted_store/readd_status.rs | `encrypted_store::readd_status::tests::test_update_requested_at_sequence_id_updates_from_null` | `xmtp_common::test`; null request updated. | `CORE-REQ-135` |
| crates/xmtp_db/src/encrypted_store/readd_status.rs | `encrypted_store::readd_status::tests::test_update_responded_at_sequence_id_creates_new` | `xmtp_common::test`; async declaration; absent row. | `CORE-REQ-135` |
| crates/xmtp_db/src/encrypted_store/readd_status.rs | `encrypted_store::readd_status::tests::test_update_responded_at_sequence_id_only_updates_if_higher` | `xmtp_common::test`; lower ignored and higher accepted. | `CORE-REQ-135` |
| crates/xmtp_db/src/encrypted_store/readd_status.rs | `encrypted_store::readd_status::tests::test_is_awaiting_readd_no_status` | `xmtp_common::test`; missing status. | `CORE-REQ-135` |
| crates/xmtp_db/src/encrypted_store/readd_status.rs | `encrypted_store::readd_status::tests::test_is_awaiting_readd_no_request` | `xmtp_common::test`; null request. | `CORE-REQ-135` |
| crates/xmtp_db/src/encrypted_store/readd_status.rs | `encrypted_store::readd_status::tests::test_is_awaiting_readd_request_pending` | `xmtp_common::test`; request greater than response. | `CORE-REQ-135` |
| crates/xmtp_db/src/encrypted_store/readd_status.rs | `encrypted_store::readd_status::tests::test_is_awaiting_readd_request_fulfilled` | `xmtp_common::test`; request lower than response. | `CORE-REQ-135` |
| crates/xmtp_db/src/encrypted_store/readd_status.rs | `encrypted_store::readd_status::tests::test_is_awaiting_readd_equal_sequence_ids` | `xmtp_common::test`; equal request and response. | `CORE-REQ-135` |
| crates/xmtp_db/src/encrypted_store/readd_status.rs | `encrypted_store::readd_status::tests::test_is_awaiting_readd_no_responded_at` | `xmtp_common::test`; null response. | `CORE-REQ-135` |
| crates/xmtp_db/src/encrypted_store/readd_status.rs | `encrypted_store::readd_status::tests::test_delete_other_readd_statuses` | `xmtp_common::test`; preserve self and another group. | `CORE-REQ-135` |
| crates/xmtp_db/src/encrypted_store/readd_status.rs | `encrypted_store::readd_status::tests::test_get_readds_awaiting_response` | `xmtp_common::test`; six status shapes. | `CORE-REQ-135` |
| crates/xmtp_db/src/encrypted_store/refresh_state.rs | `encrypted_store::refresh_state::tests::get_cursor_with_no_existing_state` | `xmtp_common::test`; missing single cursor initializes zero. | `CORE-REQ-140` |
| crates/xmtp_db/src/encrypted_store/refresh_state.rs | `encrypted_store::refresh_state::tests::get_cursor_with_no_existing_state_originator` | `xmtp_common::test`; missing batch-originator cursor initializes zero. | `CORE-REQ-140` |
| crates/xmtp_db/src/encrypted_store/refresh_state.rs | `encrypted_store::refresh_state::tests::get_timestamp_with_existing_state` | `xmtp_common::test`; existing Welcome state. | `CORE-REQ-140` |
| crates/xmtp_db/src/encrypted_store/refresh_state.rs | `encrypted_store::refresh_state::tests::update_timestamp_when_bigger` | `xmtp_common::test`; 123 to 124. | `CORE-REQ-140` |
| crates/xmtp_db/src/encrypted_store/refresh_state.rs | `encrypted_store::refresh_state::tests::dont_update_timestamp_when_smaller` | `xmtp_common::test`; 123 then 122. | `CORE-REQ-140` |
| crates/xmtp_db/src/encrypted_store/refresh_state.rs | `encrypted_store::refresh_state::tests::allow_installation_and_welcome_same_id` | `xmtp_common::test`; Welcome and Application kinds share one ID. | `CORE-REQ-140` |
| crates/xmtp_db/src/encrypted_store/refresh_state.rs | `encrypted_store::refresh_state::tests::batch_query_scenarios` | Parameterized `xmtp_common::test`; four cases: mixed existing and missing, request order, all missing, and empty. | `CORE-REQ-140` |
| crates/xmtp_db/src/encrypted_store/refresh_state.rs | `encrypted_store::refresh_state::tests::latest_cursor_for_id` | Parameterized `xmtp_common::test`; four cases: latest per originator, single, kind filter, and originator filter. | `CORE-REQ-140` |
| crates/xmtp_db/src/encrypted_store/refresh_state.rs | `encrypted_store::refresh_state::tests::get_last_cursor_for_ids_empty` | `xmtp_common::test`; empty ID list. | `CORE-REQ-140` |
| crates/xmtp_db/src/encrypted_store/refresh_state.rs | `encrypted_store::refresh_state::tests::get_last_cursor_for_ids_single` | `xmtp_common::test`; async declaration; one ID. | `CORE-REQ-140` |
| crates/xmtp_db/src/encrypted_store/refresh_state.rs | `encrypted_store::refresh_state::tests::get_last_cursor_for_ids_multiple_mixed` | `xmtp_common::test`; three existing and one missing ID. | `CORE-REQ-140` |
| crates/xmtp_db/src/encrypted_store/refresh_state.rs | `encrypted_store::refresh_state::tests::get_last_cursor_for_ids_exactly_900` | `xmtp_common::test`; loop creates exactly 900 IDs. | `CORE-REQ-140` |
| crates/xmtp_db/src/encrypted_store/refresh_state.rs | `encrypted_store::refresh_state::tests::get_last_cursor_for_ids_over_900` | `xmtp_common::test`; loop creates 1000 IDs. | `CORE-REQ-140` |
| crates/xmtp_db/src/encrypted_store/refresh_state.rs | `encrypted_store::refresh_state::tests::get_last_cursor_for_ids_over_1800` | `xmtp_common::test`; loop creates 2000 IDs. | `CORE-REQ-140` |
| crates/xmtp_db/src/encrypted_store/refresh_state.rs | `encrypted_store::refresh_state::tests::get_last_cursor_for_ids_different_entity_kinds` | `xmtp_common::test`; Application and Welcome filters. | `CORE-REQ-140` |
| crates/xmtp_db/src/encrypted_store/tasks.rs | `encrypted_store::tasks::tests::get_tasks_returns_empty_list_initially` | `xmtp_common::test`; empty table. | `CORE-REQ-145` |
| crates/xmtp_db/src/encrypted_store/tasks.rs | `encrypted_store::tasks::tests::update_task_returns_error_when_not_found` | `xmtp_common::test`; ID 999. | `CORE-REQ-145` |
| crates/xmtp_db/src/encrypted_store/tasks.rs | `encrypted_store::tasks::tests::delete_task_returns_false_when_not_found` | `xmtp_common::test`; ID 999. | `CORE-REQ-145` |
| crates/xmtp_db/src/encrypted_store/tasks.rs | `encrypted_store::tasks::tests::all_task_operations_work_together` | `xmtp_common::test`; two tasks through create, order, update, and delete. | `CORE-REQ-145` |
| crates/xmtp_db/src/encrypted_store/tasks.rs | `encrypted_store::tasks::tests::data_hash_for_matches_builder` | `xmtp_common::test`; builder and helper. | `CORE-REQ-147` |
| crates/xmtp_db/src/encrypted_store/tasks.rs | `encrypted_store::tasks::tests::data_hash_encoding_is_pinned` | `xmtp_common::test`; four proto shapes, fixed hashes, and 100 repeats. | `CORE-REQ-147` |
| crates/xmtp_db/src/encrypted_store/tasks.rs | `encrypted_store::tasks::tests::create_or_ignore_task_is_idempotent` | `xmtp_common::test`; duplicate payload. | `CORE-REQ-145` |
| crates/xmtp_db/src/encrypted_store/tasks.rs | `encrypted_store::tasks::tests::pull_in_lowers_deadline` | `xmtp_common::test`; lower, later, and missing hash. | `CORE-REQ-145` |
| crates/xmtp_db/src/encrypted_store/tasks.rs | `encrypted_store::tasks::tests::upsert_pending_self_remove_dedups_per_group` | `xmtp_common::test`; `unwrap_try`; same and different groups. | `CORE-REQ-150` |
| crates/xmtp_db/src/encrypted_store/tasks.rs | `encrypted_store::tasks::tests::upsert_preserves_live_task_but_replaces_dead_one` | `xmtp_common::test`; `unwrap_try`; live and exhausted rows. | `CORE-REQ-150` |
| crates/xmtp_db/src/encrypted_store/user_preferences.rs | `encrypted_store::user_preferences::tests::test_insert_and_update_preferences` | `xmtp_common::test`; default and one 42-byte HMAC key. | `CORE-REQ-151` |
| crates/xmtp_db/src/latency_vfs.rs | `latency_vfs::attribution::attribute_init_writes` | Native-only with `bench` feature; diagnostic test; all migrations; no value assertion. | `CORE-REQ-152` |
| crates/xmtp_db/src/latency_vfs.rs | `latency_vfs::attribution::attribute_init_op_times` | Native-only with `bench` feature; diagnostic test; optional `XMTP_BENCH_DIR`; no value assertion. | `CORE-REQ-152` |
| crates/xmtp_db/src/sql_key_store.rs | `sql_key_store::tests::store_read_delete` | `xmtp_common::test`; async declaration; read before, after write, and after delete. | `CORE-REQ-154` |
| crates/xmtp_db/src/sql_key_store.rs | `sql_key_store::tests::test_read_write` | `xmtp_common::test`; async declaration; `unwrap_try`; missing and present keys. | `CORE-REQ-154` |
| crates/xmtp_db/src/sql_key_store.rs | `sql_key_store::tests::transaction_commit_persists_rollback_does_not_and_error_propagates` | `xmtp_common::test`; async declaration; `unwrap_try`; commit, rollback, and error. | `CORE-REQ-154` |
| crates/xmtp_db/src/sql_key_store.rs | `sql_key_store::tests::list_append_remove` | `xmtp_common::test`; async declaration; ten proposals, remove index 5, and clear. | `CORE-REQ-154` |
| crates/xmtp_db/src/sql_key_store.rs | `sql_key_store::tests::group_state` | `xmtp_common::test`; async declaration. | `CORE-REQ-154` |
| crates/xmtp_db/src/sql_key_store.rs | `sql_key_store::tests::application_export_tree` | `xmtp_common::test`; async declaration; two groups, overwrite, delete, and repeated delete. | `CORE-REQ-154` |
| crates/xmtp_db/tests/opfs.rs | `test_opfs` | WASM-only integration test in a dedicated worker; OPFS cleanup. | `CORE-REQ-160` |
| crates/xmtp_db/tests/opfs.rs | `opfs_dynamically_resizes` | WASM-only integration test in a dedicated worker; four nested stores. | `CORE-REQ-160` |
