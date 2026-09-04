# MLS common test index

[← Test inventory](../existing-tests.md) · [Requirements](../existing-requirements.md)

`tls_map_tests!` applies each source template to 20 K/V cases: `(u8,u8/u16/u32/u64)`, `(u16,u8/u16/u32/u64)`, `(u32,u8/u16/u32/u64)`, `(u64,u8/u16/u32/u64)`, `(u16,[u8;32])`, `(u64,Vec<u8>)`, `([u8;32],Vec<u8>)`, and `(Vec<u8>,Vec<u8>)`.

| File | Qualified test | Form / gates / cases | Requirements |
| --- | --- | --- | --- |
| `crates/xmtp_mls_common/src/app_data/component_id.rs` | `app_data::component_id::tests::test_well_known_ids_are_in_expected_ranges` | custom sync | `OTHER-REQ-001` |
| `crates/xmtp_mls_common/src/app_data/component_id.rs` | `app_data::component_id::tests::test_range_boundaries` | custom sync | `OTHER-REQ-001` |
| `crates/xmtp_mls_common/src/app_data/component_id.rs` | `app_data::component_id::tests::test_is_in_component_space` | custom sync | `OTHER-REQ-001` |
| `crates/xmtp_mls_common/src/app_data/component_id.rs` | `app_data::component_id::tests::test_ranges_are_mutually_exclusive` | custom sync | `OTHER-REQ-001` |
| `crates/xmtp_mls_common/src/app_data/component_id.rs` | `app_data::component_id::tests::test_tls_codec_round_trip` | custom sync | `OTHER-REQ-001` |
| `crates/xmtp_mls_common/src/app_data/component_id.rs` | `app_data::component_id::tests::test_vlen_encoding_sizes` | custom sync | `OTHER-REQ-001` |
| `crates/xmtp_mls_common/src/app_data/component_id.rs` | `app_data::component_id::tests::test_decode_rejects_value_above_u16_max` | custom sync | `OTHER-REQ-001` |
| `crates/xmtp_mls_common/src/app_data/component_id.rs` | `app_data::component_id::tests::test_ordering` | custom sync | `OTHER-REQ-001` |
| `crates/xmtp_mls_common/src/app_data/component_id.rs` | `app_data::component_id::tests::test_debug_display` | custom sync | `OTHER-REQ-001` |
| `crates/xmtp_mls_common/src/app_data/component_permissions.rs` | `app_data::component_permissions::tests::test_permission_encode_decode_round_trip` | custom sync; mixed-policy object encodes and decodes equal to itself | `OTHER-REQ-004` |
| `crates/xmtp_mls_common/src/app_data/component_permissions.rs` | `app_data::component_permissions::tests::test_all_deny` | custom sync; all-Deny object encodes and decodes equal to itself | `OTHER-REQ-004` |
| `crates/xmtp_mls_common/src/app_data/component_permissions.rs` | `app_data::component_permissions::tests::test_builder_sets_all_fields` | custom sync; checks only that all three slots are present | `OTHER-REQ-004` |
| `crates/xmtp_mls_common/src/app_data/component_registry.rs` | `app_data::component_registry::tests::test_set_and_get` | custom sync | `OTHER-REQ-005` |
| `crates/xmtp_mls_common/src/app_data/component_registry.rs` | `app_data::component_registry::tests::test_get_missing_returns_none` | custom sync | `OTHER-REQ-005` |
| `crates/xmtp_mls_common/src/app_data/component_registry.rs` | `app_data::component_registry::tests::test_set_overwrites` | custom sync | `OTHER-REQ-005` |
| `crates/xmtp_mls_common/src/app_data/component_registry.rs` | `app_data::component_registry::tests::test_remove` | custom sync | `OTHER-REQ-005` |
| `crates/xmtp_mls_common/src/app_data/component_registry.rs` | `app_data::component_registry::tests::test_remove_missing_returns_error` | custom sync | `OTHER-REQ-005` |
| `crates/xmtp_mls_common/src/app_data/component_registry.rs` | `app_data::component_registry::tests::test_reject_hardcoded_set` | custom sync | `OTHER-REQ-005` |
| `crates/xmtp_mls_common/src/app_data/component_registry.rs` | `app_data::component_registry::tests::test_reject_hardcoded_remove` | custom sync | `OTHER-REQ-005` |
| `crates/xmtp_mls_common/src/app_data/component_registry.rs` | `app_data::component_registry::tests::test_admin_list_accepts_admin_or_super_admin_policy` | custom sync | `OTHER-REQ-005` |
| `crates/xmtp_mls_common/src/app_data/component_registry.rs` | `app_data::component_registry::tests::test_admin_list_accepts_super_admin_only_policy` | custom sync | `OTHER-REQ-005` |
| `crates/xmtp_mls_common/src/app_data/component_registry.rs` | `app_data::component_registry::tests::test_admin_list_rejects_allow_policy` | custom sync | `OTHER-REQ-005` |
| `crates/xmtp_mls_common/src/app_data/component_registry.rs` | `app_data::component_registry::tests::test_admin_list_rejects_deny_policy` | custom sync | `OTHER-REQ-005` |
| `crates/xmtp_mls_common/src/app_data/component_registry.rs` | `app_data::component_registry::tests::test_admin_list_rejects_mixed_invalid_policy` | custom sync | `OTHER-REQ-005` |
| `crates/xmtp_mls_common/src/app_data/component_registry.rs` | `app_data::component_registry::tests::test_admin_list_rejects_and_condition_policy` | custom sync | `OTHER-REQ-005` |
| `crates/xmtp_mls_common/src/app_data/component_registry.rs` | `app_data::component_registry::tests::test_admin_list_rejects_any_condition_policy` | custom sync | `OTHER-REQ-005` |
| `crates/xmtp_mls_common/src/app_data/component_registry.rs` | `app_data::component_registry::tests::test_rejects_missing_permissions` | custom sync | `OTHER-REQ-005` |
| `crates/xmtp_mls_common/src/app_data/component_registry.rs` | `app_data::component_registry::tests::test_rejects_missing_policy_field` | custom sync | `OTHER-REQ-005` |
| `crates/xmtp_mls_common/src/app_data/component_registry.rs` | `app_data::component_registry::tests::test_reject_reserved_set` | custom sync | `OTHER-REQ-005` |
| `crates/xmtp_mls_common/src/app_data/component_registry.rs` | `app_data::component_registry::tests::test_reject_invalid_id` | custom sync | `OTHER-REQ-005` |
| `crates/xmtp_mls_common/src/app_data/component_registry.rs` | `app_data::component_registry::tests::test_app_range_allowed` | custom sync | `OTHER-REQ-005` |
| `crates/xmtp_mls_common/src/app_data/component_registry.rs` | `app_data::component_registry::tests::test_immutable_first_insert_allowed` | custom sync | `OTHER-REQ-005` |
| `crates/xmtp_mls_common/src/app_data/component_registry.rs` | `app_data::component_registry::tests::test_immutable_subsequent_set_rejected` | custom sync | `OTHER-REQ-005` |
| `crates/xmtp_mls_common/src/app_data/component_registry.rs` | `app_data::component_registry::tests::test_immutable_remove_rejected` | custom sync | `OTHER-REQ-005` |
| `crates/xmtp_mls_common/src/app_data/component_registry.rs` | `app_data::component_registry::tests::test_tls_round_trip` | custom sync | `OTHER-REQ-005` |
| `crates/xmtp_mls_common/src/app_data/component_registry.rs` | `app_data::component_registry::tests::test_iter` | custom sync | `OTHER-REQ-005` |
| `crates/xmtp_mls_common/src/app_data/component_registry.rs` | `app_data::component_registry::tests::test_empty_registry_round_trip` | custom sync | `OTHER-REQ-005` |
| `crates/xmtp_mls_common/src/app_data/component_registry.rs` | `app_data::component_registry::tests::test_from_bytes_tolerates_out_of_space_id` | custom sync | `OTHER-REQ-005` |
| `crates/xmtp_mls_common/src/app_data/component_registry.rs` | `app_data::component_registry::tests::test_from_bytes_tolerates_reserved_id` | custom sync | `OTHER-REQ-005` |
| `crates/xmtp_mls_common/src/app_data/component_registry.rs` | `app_data::component_registry::tests::test_from_bytes_tolerates_hardcoded_id` | custom sync | `OTHER-REQ-005` |
| `crates/xmtp_mls_common/src/app_data/component_registry.rs` | `app_data::component_registry::tests::test_from_bytes_tolerates_missing_permissions` | custom sync | `OTHER-REQ-005` |
| `crates/xmtp_mls_common/src/app_data/component_registry.rs` | `app_data::component_registry::tests::test_from_bytes_tolerates_missing_policy_field` | custom sync | `OTHER-REQ-005` |
| `crates/xmtp_mls_common/src/app_data/component_registry.rs` | `app_data::component_registry::tests::test_from_bytes_tolerates_constrained_violation` | custom sync | `OTHER-REQ-005` |
| `crates/xmtp_mls_common/src/app_data/component_registry.rs` | `app_data::component_registry::tests::test_reads_skip_entry_with_undecodable_value` | custom sync | `OTHER-REQ-005` |
| `crates/xmtp_mls_common/src/app_data/component_registry.rs` | `app_data::component_registry::tests::test_from_bytes_tolerates_malformed_protobuf_value` | custom sync | `OTHER-REQ-005` |
| `crates/xmtp_mls_common/src/app_data/component_registry.rs` | `app_data::component_registry::tests::test_from_bytes_rejects_undecodable_outer_map` | custom sync | `OTHER-REQ-005` |
| `crates/xmtp_mls_common/src/app_data/component_registry.rs` | `app_data::component_registry::tests::test_from_bytes_mixed_valid_and_invalid_entries` | custom sync | `OTHER-REQ-005` |
| `crates/xmtp_mls_common/src/app_data/component_registry.rs` | `app_data::component_registry::tests::test_poisoned_registry_still_validates_unrelated_writes` | custom sync; unwrap_try | `OTHER-REQ-005` |
| `crates/xmtp_mls_common/src/app_data/component_registry.rs` | `app_data::component_registry::tests::test_set_repairs_unrecognized_entry` | custom sync | `OTHER-REQ-005` |
| `crates/xmtp_mls_common/src/app_data/component_registry.rs` | `app_data::component_registry::tests::test_remove_deletes_unrecognized_entry` | custom sync | `OTHER-REQ-005` |
| `crates/xmtp_mls_common/src/app_data/component_registry.rs` | `app_data::component_registry::tests::test_preserves_component_type` | custom sync | `OTHER-REQ-005` |
| `crates/xmtp_mls_common/src/app_data/components/inbox_id_set.rs` | `app_data::components::inbox_id_set::tests::round_trip_admin_list_value` | custom sync; unwrap_try | `OTHER-REQ-014` |
| `crates/xmtp_mls_common/src/app_data/components/inbox_id_set.rs` | `app_data::components::inbox_id_set::tests::encode_mutation_serializes_full_delta` | custom sync; unwrap_try | `OTHER-REQ-014` |
| `crates/xmtp_mls_common/src/app_data/components/inbox_id_set.rs` | `app_data::components::inbox_id_set::tests::encode_mutation_supports_batched_delta` | custom sync; unwrap_try | `OTHER-REQ-014` |
| `crates/xmtp_mls_common/src/app_data/components/inbox_id_set.rs` | `app_data::components::inbox_id_set::tests::apply_insert_against_empty_prior` | custom sync; unwrap_try | `OTHER-REQ-014` |
| `crates/xmtp_mls_common/src/app_data/components/inbox_id_set.rs` | `app_data::components::inbox_id_set::tests::apply_remove_against_existing_prior` | custom sync; unwrap_try | `OTHER-REQ-014` |
| `crates/xmtp_mls_common/src/app_data/components/inbox_id_set.rs` | `app_data::components::inbox_id_set::tests::apply_batched_delta_atomically` | custom sync; unwrap_try | `OTHER-REQ-014` |
| `crates/xmtp_mls_common/src/app_data/components/inbox_id_set.rs` | `app_data::components::inbox_id_set::tests::expand_insert_yields_single_change` | custom sync; unwrap_try | `OTHER-REQ-014` |
| `crates/xmtp_mls_common/src/app_data/components/inbox_id_set.rs` | `app_data::components::inbox_id_set::tests::expand_batched_delta_yields_one_change_per_mutation` | custom sync; unwrap_try | `OTHER-REQ-014` |
| `crates/xmtp_mls_common/src/app_data/components/inbox_id_set.rs` | `app_data::components::inbox_id_set::tests::expand_remove_by_hash_resolves_against_prior` | custom sync; unwrap_try | `OTHER-REQ-014` |
| `crates/xmtp_mls_common/src/app_data/components/inbox_id_set.rs` | `app_data::components::inbox_id_set::tests::expand_remove_by_hash_with_no_prior_yields_unresolved` | custom sync; unwrap_try | `OTHER-REQ-014` |
| `crates/xmtp_mls_common/src/app_data/components/inbox_id_set.rs` | `app_data::components::inbox_id_set::tests::apply_rejects_non_delta_payload` | custom sync; unwrap_try | `OTHER-REQ-014` |
| `crates/xmtp_mls_common/src/app_data/components/inbox_id_set.rs` | `app_data::components::inbox_id_set::tests::dm_members_uses_same_codec` | custom sync; unwrap_try | `OTHER-REQ-014` |
| `crates/xmtp_mls_common/src/app_data/components/metadata_attributes.rs` | `app_data::components::metadata_attributes::tests::string_component_round_trip` | custom sync; unwrap_try | `OTHER-REQ-018` |
| `crates/xmtp_mls_common/src/app_data/components/metadata_attributes.rs` | `app_data::components::metadata_attributes::tests::string_component_decode_rejects_non_utf8` | custom sync; unwrap_try | `OTHER-REQ-018` |
| `crates/xmtp_mls_common/src/app_data/components/metadata_attributes.rs` | `app_data::components::metadata_attributes::tests::string_component_apply_rejects_non_utf8` | custom sync; unwrap_try | `OTHER-REQ-018` |
| `crates/xmtp_mls_common/src/app_data/components/metadata_attributes.rs` | `app_data::components::metadata_attributes::tests::string_component_expand_rejects_non_utf8` | custom sync; unwrap_try | `OTHER-REQ-018` |
| `crates/xmtp_mls_common/src/app_data/components/metadata_attributes.rs` | `app_data::components::metadata_attributes::tests::commit_log_signer_round_trip` | custom sync; unwrap_try | `OTHER-REQ-019` |
| `crates/xmtp_mls_common/src/app_data/components/metadata_attributes.rs` | `app_data::components::metadata_attributes::tests::commit_log_signer_rejects_wrong_length` | custom sync; unwrap_try | `OTHER-REQ-019` |
| `crates/xmtp_mls_common/src/app_data/components/metadata_attributes.rs` | `app_data::components::metadata_attributes::tests::message_disappear_round_trip` | custom sync; unwrap_try | `OTHER-REQ-020` |
| `crates/xmtp_mls_common/src/app_data/components/metadata_attributes.rs` | `app_data::components::metadata_attributes::tests::message_disappear_rejects_wrong_length` | custom sync; unwrap_try | `OTHER-REQ-020` |
| `crates/xmtp_mls_common/src/app_data/components/metadata_attributes.rs` | `app_data::components::metadata_attributes::tests::apply_update_passthrough` | custom sync; unwrap_try | `OTHER-REQ-021` |
| `crates/xmtp_mls_common/src/app_data/components/metadata_attributes.rs` | `app_data::components::metadata_attributes::tests::expand_update_yields_one_change` | custom sync; unwrap_try | `OTHER-REQ-021` |
| `crates/xmtp_mls_common/src/app_data/components/metadata_attributes.rs` | `app_data::components::metadata_attributes::tests::expand_remove_yields_delete_with_no_value` | custom sync; unwrap_try | `OTHER-REQ-021` |
| `crates/xmtp_mls_common/src/app_data/components/metadata_attributes.rs` | `app_data::components::metadata_attributes::tests::component_id_constants_match_static_id` | custom sync; unwrap_try | `OTHER-REQ-021` |
| `crates/xmtp_mls_common/src/app_data/components/metadata_attributes.rs` | `app_data::components::metadata_attributes::tests::component_types_are_correct` | custom sync; unwrap_try | `OTHER-REQ-021` |
| `crates/xmtp_mls_common/src/app_data/components/tls_map_components.rs` | `app_data::components::tls_map_components::tests::group_membership_round_trip_value` | custom sync; unwrap_try | `OTHER-REQ-022` |
| `crates/xmtp_mls_common/src/app_data/components/tls_map_components.rs` | `app_data::components::tls_map_components::tests::group_membership_apply_insert_against_empty` | custom sync; unwrap_try | `OTHER-REQ-022` |
| `crates/xmtp_mls_common/src/app_data/components/tls_map_components.rs` | `app_data::components::tls_map_components::tests::group_membership_apply_update_against_existing` | custom sync; unwrap_try | `OTHER-REQ-022` |
| `crates/xmtp_mls_common/src/app_data/components/tls_map_components.rs` | `app_data::components::tls_map_components::tests::group_membership_apply_batched_delta_atomically` | custom sync; unwrap_try | `OTHER-REQ-022` |
| `crates/xmtp_mls_common/src/app_data/components/tls_map_components.rs` | `app_data::components::tls_map_components::tests::group_membership_expand_insert_carries_value_bytes` | custom sync; unwrap_try | `OTHER-REQ-022` |
| `crates/xmtp_mls_common/src/app_data/components/tls_map_components.rs` | `app_data::components::tls_map_components::tests::group_membership_expand_delete_carries_key_bytes` | custom sync; unwrap_try | `OTHER-REQ-022` |
| `crates/xmtp_mls_common/src/app_data/components/tls_map_components.rs` | `app_data::components::tls_map_components::tests::group_membership_expand_batched_yields_one_per_mutation` | custom sync; unwrap_try | `OTHER-REQ-022` |
| `crates/xmtp_mls_common/src/app_data/components/tls_map_components.rs` | `app_data::components::tls_map_components::tests::component_registry_round_trip` | custom sync; unwrap_try | `OTHER-REQ-022` |
| `crates/xmtp_mls_common/src/app_data/components/tls_map_components.rs` | `app_data::components::tls_map_components::tests::component_registry_apply_update_replaces_metadata` | custom sync; unwrap_try | `OTHER-REQ-022` |
| `crates/xmtp_mls_common/src/app_data/components/tls_map_components.rs` | `app_data::components::tls_map_components::tests::component_registry_apply_batched_delta_atomically` | custom sync; unwrap_try | `OTHER-REQ-022` |
| `crates/xmtp_mls_common/src/app_data/components/tls_map_components.rs` | `app_data::components::tls_map_components::tests::registry_delta_rejects_reserved_range_insert` | custom sync; unwrap_try | `OTHER-REQ-022` |
| `crates/xmtp_mls_common/src/app_data/components/tls_map_components.rs` | `app_data::components::tls_map_components::tests::registry_delta_rejects_hardcoded_insert` | custom sync; unwrap_try | `OTHER-REQ-022` |
| `crates/xmtp_mls_common/src/app_data/components/tls_map_components.rs` | `app_data::components::tls_map_components::tests::registry_delta_rejects_out_of_space_insert` | custom sync; unwrap_try | `OTHER-REQ-022` |
| `crates/xmtp_mls_common/src/app_data/components/tls_map_components.rs` | `app_data::components::tls_map_components::tests::registry_delta_rejects_undecodable_metadata_insert` | custom sync; unwrap_try | `OTHER-REQ-022` |
| `crates/xmtp_mls_common/src/app_data/components/tls_map_components.rs` | `app_data::components::tls_map_components::tests::registry_delta_rejects_immutable_update_and_delete` | custom sync; unwrap_try | `OTHER-REQ-022` |
| `crates/xmtp_mls_common/src/app_data/components/tls_map_components.rs` | `app_data::components::tls_map_components::tests::registry_delta_rejects_hardcoded_delete` | custom sync; unwrap_try | `OTHER-REQ-022` |
| `crates/xmtp_mls_common/src/app_data/components/tls_map_components.rs` | `app_data::components::tls_map_components::tests::registry_component_rejects_whole_component_remove` | custom sync; unwrap_try | `OTHER-REQ-022` |
| `crates/xmtp_mls_common/src/app_data/components/tls_map_components.rs` | `app_data::components::tls_map_components::tests::registry_delta_valid_mutations_expand_and_apply` | custom sync; unwrap_try | `OTHER-REQ-022` |
| `crates/xmtp_mls_common/src/app_data/components/tls_map_components.rs` | `app_data::components::tls_map_components::tests::apply_rejects_non_delta_payload` | custom sync; unwrap_try | `OTHER-REQ-022` |
| `crates/xmtp_mls_common/src/app_data/components/tls_map_components.rs` | `app_data::components::tls_map_components::tests::component_types_are_correct` | custom sync; unwrap_try | `OTHER-REQ-022` |
| `crates/xmtp_mls_common/src/app_data/components/type_dispatch.rs` | `app_data::components::type_dispatch::tests::apply_bytes_passthrough` | custom sync; unwrap_try | `OTHER-REQ-027` |
| `crates/xmtp_mls_common/src/app_data/components/type_dispatch.rs` | `app_data::components::type_dispatch::tests::apply_string_validates_utf8` | custom sync; unwrap_try | `OTHER-REQ-027` |
| `crates/xmtp_mls_common/src/app_data/components/type_dispatch.rs` | `app_data::components::type_dispatch::tests::apply_tls_set_inbox_id_delta` | custom sync; unwrap_try | `OTHER-REQ-027` |
| `crates/xmtp_mls_common/src/app_data/components/type_dispatch.rs` | `app_data::components::type_dispatch::tests::apply_tls_set_bytes_delta` | custom sync; unwrap_try | `OTHER-REQ-027` |
| `crates/xmtp_mls_common/src/app_data/components/type_dispatch.rs` | `app_data::components::type_dispatch::tests::apply_tls_map_inbox_id_bytes_delta` | custom sync; unwrap_try | `OTHER-REQ-027` |
| `crates/xmtp_mls_common/src/app_data/components/type_dispatch.rs` | `app_data::components::type_dispatch::tests::apply_tls_map_bytes_bytes_delta` | custom sync; unwrap_try | `OTHER-REQ-027` |
| `crates/xmtp_mls_common/src/app_data/components/type_dispatch.rs` | `app_data::components::type_dispatch::tests::unspecified_type_is_rejected` | custom sync; unwrap_try | `OTHER-REQ-027` |
| `crates/xmtp_mls_common/src/app_data/components/type_dispatch.rs` | `app_data::components::type_dispatch::tests::expand_bytes_remove_yields_delete_no_value` | custom sync; unwrap_try | `OTHER-REQ-027` |
| `crates/xmtp_mls_common/src/app_data/components/type_dispatch.rs` | `app_data::components::type_dispatch::tests::expand_tls_set_bytes_insert_yields_per_element` | custom sync; unwrap_try | `OTHER-REQ-027` |
| `crates/xmtp_mls_common/src/app_data/components/type_dispatch.rs` | `app_data::components::type_dispatch::tests::expand_tls_set_bytes_remove_by_hash_resolves_against_prior` | custom sync; unwrap_try | `OTHER-REQ-027` |
| `crates/xmtp_mls_common/src/app_data/custom.rs` | `app_data::custom::tests::rejects_out_of_range_ids` | custom sync; unwrap_try | `OTHER-REQ-029` |
| `crates/xmtp_mls_common/src/app_data/custom.rs` | `app_data::custom::tests::registers_and_dispatches_through_lookup` | custom sync; unwrap_try | `OTHER-REQ-029` |
| `crates/xmtp_mls_common/src/app_data/custom.rs` | `app_data::custom::tests::duplicate_registration_rejected` | custom sync; unwrap_try | `OTHER-REQ-029` |
| `crates/xmtp_mls_common/src/app_data/custom.rs` | `app_data::custom::tests::lookup_returns_none_for_unregistered_id` | custom sync; unwrap_try | `OTHER-REQ-029` |
| `crates/xmtp_mls_common/src/app_data/migration.rs` | `app_data::migration::tests::synthesizes_all_well_known_components` | unit | `OTHER-REQ-030` |
| `crates/xmtp_mls_common/src/app_data/migration.rs` | `app_data::migration::tests::synthesis_defaults_to_admin_for_missing_metadata_fields` | unit | `OTHER-REQ-030` |
| `crates/xmtp_mls_common/src/app_data/migration.rs` | `app_data::migration::tests::synthesis_defaults_min_version_floor_to_super_admin_when_missing` | unit | `OTHER-REQ-030` |
| `crates/xmtp_mls_common/src/app_data/migration.rs` | `app_data::migration::tests::synthesis_preserves_a_stored_min_version_policy` | unit | `OTHER-REQ-030` |
| `crates/xmtp_mls_common/src/app_data/migration.rs` | `app_data::migration::tests::synthesis_uses_per_field_policy_when_present` | unit | `OTHER-REQ-030` |
| `crates/xmtp_mls_common/src/app_data/migration.rs` | `app_data::migration::tests::synthesis_rejects_unknown_metadata_field` | unit | `OTHER-REQ-030` |
| `crates/xmtp_mls_common/src/app_data/migration.rs` | `app_data::migration::tests::synthesis_rejects_non_super_admin_update_permissions` | unit | `OTHER-REQ-030` |
| `crates/xmtp_mls_common/src/app_data/migration.rs` | `app_data::migration::tests::synthesis_admin_list_super_admin_only` | unit | `OTHER-REQ-030` |
| `crates/xmtp_mls_common/src/app_data/migration.rs` | `app_data::migration::tests::metadata_field_mapping_agrees_with_dispatch_table` | custom sync; unwrap_try | `OTHER-REQ-030` |
| `crates/xmtp_mls_common/src/app_data/migration.rs` | `app_data::migration::tests::synthesis_sets_correct_component_type_per_field` | unit | `OTHER-REQ-030` |
| `crates/xmtp_mls_common/src/app_data/migration.rs` | `app_data::migration::tests::synthesis_deterministic_bytes` | unit | `OTHER-REQ-030` |
| `crates/xmtp_mls_common/src/app_data/migration.rs` | `app_data::migration::tests::membership_policy_rejects_combinator` | unit | `OTHER-REQ-030` |
| `crates/xmtp_mls_common/src/app_data/migration.rs` | `app_data::migration::tests::membership_policy_rejects_unknown_base` | unit | `OTHER-REQ-030` |
| `crates/xmtp_mls_common/src/app_data/migration.rs` | `app_data::migration::tests::group_membership_encode_round_trip` | unit | `OTHER-REQ-034` |
| `crates/xmtp_mls_common/src/app_data/migration.rs` | `app_data::migration::tests::admin_list_policy_rejects_combinator` | unit | `OTHER-REQ-030` |
| `crates/xmtp_mls_common/src/app_data/migration.rs` | `app_data::migration::tests::decode_wire_rejects_unset_version` | unit | `OTHER-REQ-034` |
| `crates/xmtp_mls_common/src/app_data/migration.rs` | `app_data::migration::tests::decode_wire_rejects_non_insert_mutation` | unit | `OTHER-REQ-034` |
| `crates/xmtp_mls_common/src/app_data/migration.rs` | `app_data::migration::tests::decode_dict_round_trips` | unit | `OTHER-REQ-034` |
| `crates/xmtp_mls_common/src/app_data/migration.rs` | `app_data::migration::tests::encode_inbox_id_set_emits_bootstrap_wire_delta` | unit | `OTHER-REQ-035` |
| `crates/xmtp_mls_common/src/app_data/migration.rs` | `app_data::migration::tests::encode_inbox_id_set_rejects_bad_hex` | unit | `OTHER-REQ-035` |
| `crates/xmtp_mls_common/src/app_data/migration.rs` | `app_data::migration::tests::encode_dm_members_produces_two_insert_delta` | unit | `OTHER-REQ-035` |
| `crates/xmtp_mls_common/src/app_data/migration.rs` | `app_data::migration::tests::encode_dm_members_rejects_self_reference` | unit | `OTHER-REQ-035` |
| `crates/xmtp_mls_common/src/app_data/migration.rs` | `app_data::migration::tests::encode_dm_members_rejects_case_divergent_self_reference` | unit | `OTHER-REQ-035` |
| `crates/xmtp_mls_common/src/app_data/migration.rs` | `app_data::migration::tests::conversation_type_codec_round_trips` | unit | `OTHER-REQ-036` |
| `crates/xmtp_mls_common/src/app_data/migration.rs` | `app_data::migration::tests::conversation_type_decode_rejects_wrong_length` | unit | `OTHER-REQ-036` |
| `crates/xmtp_mls_common/src/app_data/migration.rs` | `app_data::migration::tests::canonical_subset_empty_group_omits_optional_seeds` | unit | `OTHER-REQ-037` |
| `crates/xmtp_mls_common/src/app_data/migration.rs` | `app_data::migration::tests::canonical_subset_dm_group_includes_dm_members` | unit | `OTHER-REQ-037` |
| `crates/xmtp_mls_common/src/app_data/migration.rs` | `app_data::migration::tests::canonical_subset_oneshot_group_includes_oneshot` | unit | `OTHER-REQ-037` |
| `crates/xmtp_mls_common/src/app_data/migration.rs` | `app_data::migration::tests::canonical_subset_membership_sequence_ids` | unit | `OTHER-REQ-037` |
| `crates/xmtp_mls_common/src/app_data/migration.rs` | `app_data::migration::tests::canonical_subset_collects_legacy_failed_installations_into_allow_set` | unit | `OTHER-REQ-037` |
| `crates/xmtp_mls_common/src/app_data/migration.rs` | `app_data::migration::tests::canonical_subset_rejects_non_32_byte_failed_installation` | unit | `OTHER-REQ-037` |
| `crates/xmtp_mls_common/src/app_data/migration.rs` | `app_data::migration::tests::golden_bootstrap_synthesis_group` | unit | `OTHER-REQ-037` |
| `crates/xmtp_mls_common/src/app_data/migration.rs` | `app_data::migration::tests::golden_bootstrap_synthesis_dm_with_oneshot` | unit | `OTHER-REQ-037` |
| `crates/xmtp_mls_common/src/app_data/migration.rs` | `app_data::migration::tests::canonical_subset_deterministic_across_calls` | unit | `OTHER-REQ-037` |
| `crates/xmtp_mls_common/src/app_data/registry_table.rs` | `app_data::registry_table::tests::lookup_returns_correct_component_for_each_well_known_id` | custom sync; unwrap_try | `OTHER-REQ-040` |
| `crates/xmtp_mls_common/src/app_data/registry_table.rs` | `app_data::registry_table::tests::lookup_returns_none_for_unknown_id` | custom sync; unwrap_try | `OTHER-REQ-040` |
| `crates/xmtp_mls_common/src/app_data/registry_table.rs` | `app_data::registry_table::tests::well_known_entries_match_component_const_id` | custom sync; unwrap_try | `OTHER-REQ-040` |
| `crates/xmtp_mls_common/src/app_data/registry_table.rs` | `app_data::registry_table::tests::well_known_count_matches_plan` | custom sync; unwrap_try | `OTHER-REQ-040` |
| `crates/xmtp_mls_common/src/app_data/registry_table.rs` | `app_data::registry_table::tests::dispatch_through_erased_calls_typed_apply` | custom sync; unwrap_try | `OTHER-REQ-040` |
| `crates/xmtp_mls_common/src/app_data/typed.rs` | `app_data::typed::tests::typed_methods_round_trip` | custom sync; unwrap_try | `OTHER-REQ-042` |
| `crates/xmtp_mls_common/src/app_data/typed.rs` | `app_data::typed::tests::erased_component_blanket_impl` | custom sync; unwrap_try | `OTHER-REQ-042` |
| `crates/xmtp_mls_common/src/app_data/typed.rs` | `app_data::typed::tests::default_validate_invariant_is_noop` | custom sync; unwrap_try | `OTHER-REQ-042` |
| `crates/xmtp_mls_common/src/app_data/validation.rs` | `app_data::validation::tests::test_immutable_insert_allowed` | custom sync | `OTHER-REQ-043` |
| `crates/xmtp_mls_common/src/app_data/validation.rs` | `app_data::validation::tests::test_immutable_update_rejected` | custom sync | `OTHER-REQ-043` |
| `crates/xmtp_mls_common/src/app_data/validation.rs` | `app_data::validation::tests::test_immutable_delete_rejected` | custom sync | `OTHER-REQ-043` |
| `crates/xmtp_mls_common/src/app_data/validation.rs` | `app_data::validation::tests::test_registry_super_admin_allowed` | custom sync | `OTHER-REQ-043` |
| `crates/xmtp_mls_common/src/app_data/validation.rs` | `app_data::validation::tests::test_registry_admin_rejected` | custom sync | `OTHER-REQ-043` |
| `crates/xmtp_mls_common/src/app_data/validation.rs` | `app_data::validation::tests::test_registry_member_rejected` | custom sync | `OTHER-REQ-043` |
| `crates/xmtp_mls_common/src/app_data/validation.rs` | `app_data::validation::tests::test_super_admin_list_super_admin_allowed` | custom sync | `OTHER-REQ-043` |
| `crates/xmtp_mls_common/src/app_data/validation.rs` | `app_data::validation::tests::test_super_admin_list_admin_rejected` | custom sync | `OTHER-REQ-043` |
| `crates/xmtp_mls_common/src/app_data/validation.rs` | `app_data::validation::tests::test_admin_list_with_admin_policy_admin_allowed` | custom sync | `OTHER-REQ-043` |
| `crates/xmtp_mls_common/src/app_data/validation.rs` | `app_data::validation::tests::test_admin_list_with_admin_policy_member_rejected` | custom sync | `OTHER-REQ-043` |
| `crates/xmtp_mls_common/src/app_data/validation.rs` | `app_data::validation::tests::test_admin_list_with_super_admin_policy` | custom sync | `OTHER-REQ-043` |
| `crates/xmtp_mls_common/src/app_data/validation.rs` | `app_data::validation::tests::test_deny_by_default_no_entry` | custom sync | `OTHER-REQ-043` |
| `crates/xmtp_mls_common/src/app_data/validation.rs` | `app_data::validation::tests::test_insert_allow_policy` | custom sync | `OTHER-REQ-043` |
| `crates/xmtp_mls_common/src/app_data/validation.rs` | `app_data::validation::tests::test_update_admin_only_policy_admin_passes` | custom sync | `OTHER-REQ-043` |
| `crates/xmtp_mls_common/src/app_data/validation.rs` | `app_data::validation::tests::test_update_admin_only_policy_member_fails` | custom sync | `OTHER-REQ-043` |
| `crates/xmtp_mls_common/src/app_data/validation.rs` | `app_data::validation::tests::test_delete_deny_policy` | custom sync | `OTHER-REQ-043` |
| `crates/xmtp_mls_common/src/app_data/validation.rs` | `app_data::validation::tests::test_delete_super_admin_only_policy` | custom sync | `OTHER-REQ-043` |
| `crates/xmtp_mls_common/src/app_data/validation.rs` | `app_data::validation::tests::test_different_insert_vs_update_permissions` | custom sync | `OTHER-REQ-043` |
| `crates/xmtp_mls_common/src/app_data/validation.rs` | `app_data::validation::tests::test_app_range_component` | custom sync | `OTHER-REQ-043` |
| `crates/xmtp_mls_common/src/group_metadata.rs` | `group_metadata::tests::test_dm_members_sort` | custom sync | `OTHER-REQ-046` |
| `crates/xmtp_mls_common/src/group_mutable_metadata.rs` | `group_mutable_metadata::tests::test_commit_log_signer_utility_method` | unit | `OTHER-REQ-047` |
| `crates/xmtp_mls_common/src/group_mutable_metadata.rs` | `group_mutable_metadata::tests::test_lossy_merge_applies_good_fields_and_reports_bad_ones` | custom sync | `OTHER-REQ-048` |
| `crates/xmtp_mls_common/src/inbox_id.rs` | `inbox_id::tests::test_tls_serialize_writes_version_prefix_then_payload` | custom sync | `OTHER-REQ-049` |
| `crates/xmtp_mls_common/src/inbox_id.rs` | `inbox_id::tests::test_from_hex_non_hex_input` | custom sync | `OTHER-REQ-049` |
| `crates/xmtp_mls_common/src/inbox_id.rs` | `inbox_id::tests::test_tls_deserialize_rejects_non_minimal_version_zero` | custom sync | `OTHER-REQ-049` |
| `crates/xmtp_mls_common/src/inbox_id.rs` | `inbox_id::tests::hex_round_trip` | property; arbitrary 32-byte IDs | `OTHER-REQ-049` |
| `crates/xmtp_mls_common/src/inbox_id.rs` | `inbox_id::tests::tls_round_trip` | property; arbitrary 32-byte IDs | `OTHER-REQ-049` |
| `crates/xmtp_mls_common/src/inbox_id.rs` | `inbox_id::tests::size_matches_serialized_bytes` | property; arbitrary 32-byte IDs | `OTHER-REQ-049` |
| `crates/xmtp_mls_common/src/inbox_id.rs` | `inbox_id::tests::hex_and_from_bytes_agree` | property; arbitrary 32-byte IDs | `OTHER-REQ-049` |
| `crates/xmtp_mls_common/src/inbox_id.rs` | `inbox_id::tests::ord_matches_byte_order` | property; arbitrary pairs of 32-byte IDs | `OTHER-REQ-049` |
| `crates/xmtp_mls_common/src/inbox_id.rs` | `inbox_id::tests::from_hex_rejects_wrong_length` | property; 0–64-byte valid hex except 32 | `OTHER-REQ-049` |
| `crates/xmtp_mls_common/src/inbox_id.rs` | `inbox_id::tests::tls_deserialize_rejects_unsupported_version` | property; versions 1–63 | `OTHER-REQ-049` |
| `crates/xmtp_mls_common/src/inbox_id.rs` | `inbox_id::tests::formatting_is_hex` | property; arbitrary 32-byte IDs | `OTHER-REQ-049` |
| `crates/xmtp_mls_common/src/inbox_id.rs` | `inbox_id::tests::tls_set_round_trip` | property; sets of 0–15 arbitrary 32-byte IDs | `OTHER-REQ-049` |
| `crates/xmtp_mls_common/src/invite/payload.rs` | `invite::payload::tests::key_nonce_and_id_are_random` | custom sync; unwrap_try; size plus probabilistic nonzero and two-sample inequality checks | `OTHER-REQ-051` |
| `crates/xmtp_mls_common/src/invite/payload.rs` | `invite::payload::tests::validate_accepts_well_formed_v1` | custom sync; unwrap_try | `OTHER-REQ-052` |
| `crates/xmtp_mls_common/src/invite/payload.rs` | `invite::payload::tests::validate_rejects_missing_version` | custom sync; unwrap_try | `OTHER-REQ-052` |
| `crates/xmtp_mls_common/src/invite/payload.rs` | `invite::payload::tests::build_payload_rejects_short_external_group_id` | custom sync; unwrap_try | `OTHER-REQ-052` |
| `crates/xmtp_mls_common/src/invite/payload.rs` | `invite::payload::tests::validate_rejects_short_external_group_id_from_wire` | custom sync; unwrap_try | `OTHER-REQ-052` |
| `crates/xmtp_mls_common/src/invite/payload.rs` | `invite::payload::tests::validate_rejects_wrong_symmetric_key_length` | custom sync; unwrap_try | `OTHER-REQ-052` |
| `crates/xmtp_mls_common/src/invite/payload.rs` | `invite::payload::tests::build_payload_round_trip` | custom sync; unwrap_try | `OTHER-REQ-052` |
| `crates/xmtp_mls_common/src/mls_ext/payload_encryption.rs` | `mls_ext::payload_encryption::tests::round_trip_curve25519_hpke` | custom sync | `OTHER-REQ-053` |
| `crates/xmtp_mls_common/src/mls_ext/payload_encryption.rs` | `mls_ext::payload_encryption::tests::round_trip_xwing_hpke` | custom sync | `OTHER-REQ-053` |
| `crates/xmtp_mls_common/src/mls_ext/payload_encryption.rs` | `mls_ext::payload_encryption::tests::wrong_key_fails_curve25519` | custom sync | `OTHER-REQ-053` |
| `crates/xmtp_mls_common/src/mls_ext/payload_encryption.rs` | `mls_ext::payload_encryption::tests::wrong_label_fails_curve25519` | custom sync | `OTHER-REQ-053` |
| `crates/xmtp_mls_common/src/mls_ext/payload_encryption.rs` | `mls_ext::payload_encryption::tests::welcome_label_round_trip_matches_xmtp_configuration` | custom sync | `OTHER-REQ-053` |
| `crates/xmtp_mls_common/src/mls_ext/payload_encryption.rs` | `mls_ext::payload_encryption::tests::round_trip_symmetric` | custom sync | `OTHER-REQ-055` |
| `crates/xmtp_mls_common/src/mls_ext/payload_encryption.rs` | `mls_ext::payload_encryption::tests::symmetric_wrong_key_fails` | custom sync | `OTHER-REQ-055` |
| `crates/xmtp_mls_common/src/tls_map.rs` | `tls_map::tests::tls_map_tests!::round_trip` | property macro template; 20 K/V cases | `OTHER-REQ-056` |
| `crates/xmtp_mls_common/src/tls_map.rs` | `tls_map::tests::tls_map_tests!::deterministic` | property macro template; 20 K/V cases | `OTHER-REQ-056` |
| `crates/xmtp_mls_common/src/tls_map.rs` | `tls_map::tests::tls_map_tests!::insertion_order_irrelevant` | property macro template; 20 K/V cases | `OTHER-REQ-056` |
| `crates/xmtp_mls_common/src/tls_map.rs` | `tls_map::tests::tls_map_tests!::keys_always_sorted` | property macro template; 20 K/V cases | `OTHER-REQ-056` |
| `crates/xmtp_mls_common/src/tls_map.rs` | `tls_map::tests::tls_map_tests!::get_returns_inserted` | property macro template; 20 K/V cases | `OTHER-REQ-056` |
| `crates/xmtp_mls_common/src/tls_map.rs` | `tls_map::tests::tls_map_tests!::insert_duplicate_fails` | property macro template; 20 K/V cases | `OTHER-REQ-056` |
| `crates/xmtp_mls_common/src/tls_map.rs` | `tls_map::tests::tls_map_tests!::set_upsert` | property macro template; 20 K/V cases | `OTHER-REQ-056` |
| `crates/xmtp_mls_common/src/tls_map.rs` | `tls_map::tests::tls_map_tests!::remove_returns_value` | property macro template; 20 K/V cases | `OTHER-REQ-056` |
| `crates/xmtp_mls_common/src/tls_map.rs` | `tls_map::tests::tls_map_tests!::serialized_size_matches_trait` | property macro template; 20 K/V cases | `OTHER-REQ-056` |
| `crates/xmtp_mls_common/src/tls_map.rs` | `tls_map::tests::tls_map_tests!::from_pairs_and_collect_and_set_equivalent` | property macro template; 20 K/V cases | `OTHER-REQ-056` |
| `crates/xmtp_mls_common/src/tls_map.rs` | `tls_map::tests::tls_map_tests!::delta_rollback_on_failure` | property macro template; 20 K/V cases | `OTHER-REQ-056` |
| `crates/xmtp_mls_common/src/tls_map.rs` | `tls_map::tests::tls_map_tests!::into_iter_sorted` | property macro template; 20 K/V cases | `OTHER-REQ-056` |
| `crates/xmtp_mls_common/src/tls_map.rs` | `tls_map::tests::tls_map_tests!::single_entry_round_trip` | property macro template; 20 K/V cases | `OTHER-REQ-056` |
| `crates/xmtp_mls_common/src/tls_map.rs` | `tls_map::tests::tls_map_tests!::rejects_unsorted` | property macro template; 20 K/V cases | `OTHER-REQ-056` |
| `crates/xmtp_mls_common/src/tls_map.rs` | `tls_map::tests::tls_map_tests!::rejects_duplicates` | property macro template; 20 K/V cases | `OTHER-REQ-056` |
| `crates/xmtp_mls_common/src/tls_map.rs` | `tls_map::tests::tls_map_tests!::mutation_round_trip` | property macro template; 20 K/V cases | `OTHER-REQ-056` |
| `crates/xmtp_mls_common/src/tls_map.rs` | `tls_map::tests::tls_map_tests!::delta_apply_sequence` | property macro template; 20 K/V cases | `OTHER-REQ-056` |
| `crates/xmtp_mls_common/src/tls_map.rs` | `tls_map::tests::tls_map_tests!::nested_map_round_trip` | property macro template; 20 K/V cases | `OTHER-REQ-056` |
| `crates/xmtp_mls_common/src/tls_map.rs` | `tls_map::tests::tls_map_tests!::nested_map_reverse_round_trip` | property macro template; 20 K/V cases | `OTHER-REQ-056` |
| `crates/xmtp_mls_common/src/tls_map.rs` | `tls_map::tests::tls_map_tests!::delta_round_trip` | property macro template; 20 K/V cases | `OTHER-REQ-056` |
| `crates/xmtp_mls_common/src/tls_map.rs` | `tls_map::tests::tls_map_tests!::empty_map_serializes_to_zero_length_prefix` | unit macro template; 20 K/V cases | `OTHER-REQ-056` |
| `crates/xmtp_mls_common/src/tls_map.rs` | `tls_map::tests::tls_map_tests!::debug_format` | unit macro template; 20 K/V cases | `OTHER-REQ-056` |
| `crates/xmtp_mls_common/src/tls_map.rs` | `tls_map::tests::tls_map_tests!::get_mut_modifies_value` | unit macro template; 20 K/V cases | `OTHER-REQ-056` |
| `crates/xmtp_mls_common/src/tls_map.rs` | `tls_map::tests::tls_map_tests!::iter_yields_all_pairs` | unit macro template; 20 K/V cases | `OTHER-REQ-056` |
| `crates/xmtp_mls_common/src/tls_map.rs` | `tls_map::tests::tls_map_tests!::values_yields_all_values` | unit macro template; 20 K/V cases | `OTHER-REQ-056` |
| `crates/xmtp_mls_common/src/tls_map.rs` | `tls_map::tests::tls_map_tests!::default_creates_empty` | unit macro template; 20 K/V cases | `OTHER-REQ-056` |
| `crates/xmtp_mls_common/src/tls_map.rs` | `tls_map::tests::tls_map_tests!::rejects_invalid_mutation_tag` | unit macro template; 20 K/V cases | `OTHER-REQ-056` |
| `crates/xmtp_mls_common/src/tls_map.rs` | `tls_map::tests::tls_map_tests!::rejects_unsorted_deterministic` | unit macro template; 20 K/V cases | `OTHER-REQ-056` |
| `crates/xmtp_mls_common/src/tls_map.rs` | `tls_map::tests::tls_map_tests!::rejects_duplicates_deterministic` | unit macro template; 20 K/V cases | `OTHER-REQ-056` |
| `crates/xmtp_mls_common/src/tls_map.rs` | `tls_map::tests::tls_map_tests!::rejects_trailing_bytes` | unit macro template; 20 K/V cases | `OTHER-REQ-056` |
| `crates/xmtp_mls_common/src/tls_set.rs` | `tls_set::tests::test_insert_and_contains` | custom sync | `OTHER-REQ-060` |
| `crates/xmtp_mls_common/src/tls_set.rs` | `tls_set::tests::test_insert_duplicate_fails` | custom sync | `OTHER-REQ-060` |
| `crates/xmtp_mls_common/src/tls_set.rs` | `tls_set::tests::test_remove` | custom sync | `OTHER-REQ-060` |
| `crates/xmtp_mls_common/src/tls_set.rs` | `tls_set::tests::test_remove_missing_fails` | custom sync | `OTHER-REQ-060` |
| `crates/xmtp_mls_common/src/tls_set.rs` | `tls_set::tests::test_from_keys_deduplicates` | custom sync | `OTHER-REQ-060` |
| `crates/xmtp_mls_common/src/tls_set.rs` | `tls_set::tests::test_iter_sorted` | custom sync | `OTHER-REQ-060` |
| `crates/xmtp_mls_common/src/tls_set.rs` | `tls_set::tests::test_tls_round_trip` | custom sync | `OTHER-REQ-060` |
| `crates/xmtp_mls_common/src/tls_set.rs` | `tls_set::tests::test_empty_round_trip` | custom sync | `OTHER-REQ-060` |
| `crates/xmtp_mls_common/src/tls_set.rs` | `tls_set::tests::test_into_iter` | custom sync | `OTHER-REQ-060` |
| `crates/xmtp_mls_common/src/tls_set.rs` | `tls_set::tests::test_collect` | custom sync | `OTHER-REQ-060` |
| `crates/xmtp_mls_common/src/tls_set.rs` | `tls_set::tests::test_apply_delta` | custom sync | `OTHER-REQ-060` |
| `crates/xmtp_mls_common/src/tls_set.rs` | `tls_set::tests::test_apply_delta_rollback_on_failure` | custom sync | `OTHER-REQ-060` |
| `crates/xmtp_mls_common/src/tls_set.rs` | `tls_set::tests::test_remove_by_hash` | custom sync | `OTHER-REQ-060` |
| `crates/xmtp_mls_common/src/tls_set.rs` | `tls_set::tests::test_remove_by_hash_not_found` | custom sync | `OTHER-REQ-060` |
| `crates/xmtp_mls_common/src/tls_set.rs` | `tls_set::tests::test_remove_by_hash_matches_remove_by_value` | custom sync | `OTHER-REQ-060` |
| `crates/xmtp_mls_common/src/tls_set.rs` | `tls_set::tests::test_delta_tls_round_trip` | custom sync | `OTHER-REQ-060` |
| `crates/xmtp_mls_common/src/tls_set.rs` | `tls_set::tests::test_remove_by_hash_mutation_tls_round_trip` | custom sync | `OTHER-REQ-060` |
| `crates/xmtp_mls_common/src/tls_set.rs` | `tls_set::tests::test_mutation_tls_round_trip` | custom sync | `OTHER-REQ-060` |
| `crates/xmtp_mls_common/src/tls_set.rs` | `tls_set::tests::test_deserialize_unknown_tag` | custom sync | `OTHER-REQ-060` |
| `crates/xmtp_mls_common/src/tls_set.rs` | `tls_set::tests::test_apply_delta_duplicate_hash` | custom sync | `OTHER-REQ-060` |
| `crates/xmtp_mls_common/src/tls_set.rs` | `tls_set::tests::test_apply_delta_remove_by_hash_not_found_in_index` | custom sync | `OTHER-REQ-060` |
| `crates/xmtp_mls_common/src/app_data/component_permissions.rs` | `app_data::component_permissions::component_permissions (doctest line 12)` | rustdoc; ignored | `OTHER-REQ-004` |
| `crates/xmtp_mls_common/src/app_data/validation.rs` | `app_data::validation::ComponentChange (doctest line 38)` | rustdoc | `OTHER-REQ-043` |
| `crates/xmtp_mls_common/src/tls_map.rs` | `tls_map::TlsMap (doctest line 99)` | rustdoc | `OTHER-REQ-056` |
| `crates/xmtp_mls_common/src/tls_map.rs` | `tls_map::TlsMap<K,V>::apply_delta (doctest line 471)` | rustdoc; mixed atomic delta | `OTHER-REQ-056` |
| `crates/xmtp_mls_common/src/tls_map.rs` | `tls_map::TlsMap<K,V>::from_pairs (doctest line 148)` | rustdoc; duplicate key is last-wins | `OTHER-REQ-056` |
| `crates/xmtp_mls_common/src/tls_map.rs` | `tls_map::TlsMap<K,V>::get (doctest line 247)` | rustdoc; present and absent key | `OTHER-REQ-056` |
| `crates/xmtp_mls_common/src/tls_map.rs` | `tls_map::TlsMap<K,V>::get_mut (doctest line 265)` | rustdoc; mutate existing value | `OTHER-REQ-056` |
| `crates/xmtp_mls_common/src/tls_map.rs` | `tls_map::TlsMap<K,V>::insert (doctest line 168)` | rustdoc; new and duplicate key | `OTHER-REQ-056` |
| `crates/xmtp_mls_common/src/tls_map.rs` | `tls_map::TlsMap<K,V>::iter (doctest line 301)` | rustdoc; sorted pair order | `OTHER-REQ-056` |
| `crates/xmtp_mls_common/src/tls_map.rs` | `tls_map::TlsMap<K,V>::new (doctest line 133)` | rustdoc; empty map | `OTHER-REQ-056` |
| `crates/xmtp_mls_common/src/tls_map.rs` | `tls_map::TlsMap<K,V>::remove (doctest line 229)` | rustdoc; present then missing key | `OTHER-REQ-056` |
| `crates/xmtp_mls_common/src/tls_map.rs` | `tls_map::TlsMap<K,V>::set (doctest line 210)` | rustdoc; insert then overwrite | `OTHER-REQ-056` |
| `crates/xmtp_mls_common/src/tls_map.rs` | `tls_map::TlsMap<K,V>::update (doctest line 188)` | rustdoc; present and missing key | `OTHER-REQ-056` |
| `crates/xmtp_mls_common/src/tls_map.rs` | `tls_map::TlsMapDelta (doctest line 412)` | rustdoc; fluent insert/update/delete | `OTHER-REQ-056` |
| `crates/xmtp_mls_common/src/tls_set.rs` | `tls_set::TlsSet (doctest line 92)` | rustdoc; deterministic codec round-trip | `OTHER-REQ-060` |
| `crates/xmtp_mls_common/src/tls_set.rs` | `tls_set::TlsSet<K>::apply_delta (doctest line 225)` | rustdoc; insert/remove | `OTHER-REQ-060` |
| `crates/xmtp_mls_common/src/tls_set.rs` | `tls_set::TlsSet<K>::from_keys (doctest line 128)` | rustdoc; unsorted input with duplicate | `OTHER-REQ-060` |
| `crates/xmtp_mls_common/src/tls_set.rs` | `tls_set::TlsSet<K>::insert (doctest line 143)` | rustdoc; new and duplicate key | `OTHER-REQ-060` |
| `crates/xmtp_mls_common/src/tls_set.rs` | `tls_set::TlsSet<K>::iter (doctest line 191)` | rustdoc; sorted order | `OTHER-REQ-060` |
| `crates/xmtp_mls_common/src/tls_set.rs` | `tls_set::TlsSet<K>::remove (doctest line 157)` | rustdoc; present then missing key | `OTHER-REQ-060` |
