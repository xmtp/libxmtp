# Inline MLS group test index

[← Test inventory](../existing-tests.md) · [Requirements](../existing-requirements.md)

| File | Qualified test | Form / gates / cases | Requirements |
| --- | --- | --- | --- |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::test_inbox_id_round_trip` | custom sync | `GINLINE-REQ-001` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::test_inbox_id_invalid_hex` | custom sync | `GINLINE-REQ-001` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::test_inbox_id_wrong_length` | custom sync | `GINLINE-REQ-001` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::test_component_type_string_family` | custom sync | `GINLINE-REQ-002` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::test_component_type_bytes_family` | custom sync | `GINLINE-REQ-002` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::test_component_type_set_inbox_id_family` | custom sync | `GINLINE-REQ-002` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::test_component_type_group_membership` | custom sync | `GINLINE-REQ-002` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::test_component_type_app_range_is_none` | custom sync | `GINLINE-REQ-002` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::test_metadata_field_round_trip` | custom sync | `GINLINE-REQ-003` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::test_metadata_field_unknown_name_returns_none` | custom sync | `GINLINE-REQ-003` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::test_component_id_to_metadata_field_non_gmm_returns_none` | custom sync | `GINLINE-REQ-003` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::test_encode_bytes_payload_passthrough` | custom sync | `GINLINE-REQ-004` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::test_encode_bytes_payload_rejects_non_bytes_component` | custom sync | `GINLINE-REQ-004` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::test_encode_admin_list_insert_delta` | custom sync | `GINLINE-REQ-004` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::test_encode_admin_list_remove_delta` | custom sync | `GINLINE-REQ-004` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::test_encode_super_admin_list_delta` | custom sync | `GINLINE-REQ-004` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::test_encode_admin_list_invalid_inbox_id` | custom sync | `GINLINE-REQ-001`, `GINLINE-REQ-004` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::test_apply_bytes_payload_returns_payload_verbatim` | custom sync | `GINLINE-REQ-006` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::test_apply_bytes_payload_ignores_old_value` | custom sync | `GINLINE-REQ-006` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::test_apply_admin_list_insert_against_none` | custom sync | `GINLINE-REQ-006` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::test_apply_admin_list_insert_against_existing_set` | custom sync | `GINLINE-REQ-006` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::test_apply_admin_list_remove_against_existing_set` | custom sync | `GINLINE-REQ-006` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::test_apply_super_admin_list_delta` | custom sync | `GINLINE-REQ-006` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::test_apply_admin_list_malformed_payload_returns_tls_codec_error` | custom sync | `GINLINE-REQ-006` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::test_apply_admin_list_malformed_old_value_returns_tls_codec_error` | custom sync | `GINLINE-REQ-006` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::test_apply_immutable_first_insert_allowed` | custom sync | `GINLINE-REQ-006` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::test_apply_immutable_overwrite_rejected` | custom sync | `GINLINE-REQ-006` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::test_apply_component_registry_delta_against_empty` | custom sync | `GINLINE-REQ-006` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::test_apply_group_membership_delta_against_existing_map` | custom sync | `GINLINE-REQ-006` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::test_apply_map_component_malformed_delta_returns_codec_error` | custom sync | `GINLINE-REQ-006` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::test_apply_map_component_apply_failure_surfaces_apply_error` | custom sync | `GINLINE-REQ-006` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::test_apply_unknown_id_with_bytes_registry_entry_stores_payload` | custom sync | `GINLINE-REQ-006` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::test_apply_unknown_id_with_string_registry_entry_validates_utf8` | custom sync | `GINLINE-REQ-006` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::test_apply_unknown_id_with_tls_set_inbox_id_applies_delta` | custom sync | `GINLINE-REQ-006` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::test_apply_unknown_id_with_no_registry_entry_rejected` | custom sync | `GINLINE-REQ-006` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::test_apply_unknown_id_in_xmtp_immutable_range_first_write_allowed` | custom sync | `GINLINE-REQ-006` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::test_apply_unknown_id_in_xmtp_immutable_range_overwrite_rejected` | custom sync | `GINLINE-REQ-006` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::test_apply_unknown_id_in_app_immutable_range_overwrite_rejected` | custom sync | `GINLINE-REQ-006` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::test_apply_reserved_range_component_rejected_with_or_without_registry` | custom sync | `GINLINE-REQ-006` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::test_apply_out_of_range_component_rejected` | custom sync | `GINLINE-REQ-006` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::test_apply_unknown_xmtp_range_id_with_registry_dispatches` | custom sync | `GINLINE-REQ-006` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::test_expand_insert_surfaces_inbox_id_bytes` | custom sync | `GINLINE-REQ-013` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::test_expand_remove_surfaces_inbox_id_bytes` | custom sync | `GINLINE-REQ-013` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::test_expand_remove_by_hash_resolves_to_inbox_id_from_old_value` | custom sync | `GINLINE-REQ-013` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::test_expand_remove_by_hash_miss_surfaces_none_value` | custom sync | `GINLINE-REQ-013` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::test_expand_remove_by_hash_with_no_old_value_surfaces_none` | custom sync | `GINLINE-REQ-013` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::test_expand_remove_by_hash_malformed_old_value_surfaces_codec_error` | custom sync | `GINLINE-REQ-013` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::test_expand_unknown_component_bytes_typed_emits_single_update` | custom sync | `GINLINE-REQ-013` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::test_expand_unknown_component_remove_emits_single_delete` | custom sync | `GINLINE-REQ-013` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::test_expand_unknown_component_tls_set_inbox_id_emits_per_element_changes` | custom sync | `GINLINE-REQ-013` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::test_expand_reserved_range_id_still_rejected` | custom sync | `GINLINE-REQ-006`, `GINLINE-REQ-013` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::test_expand_unknown_component_no_registry_entry_rejected` | custom sync | `GINLINE-REQ-006`, `GINLINE-REQ-013` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::test_expand_skips_old_value_decode_when_no_remove_by_hash` | custom sync | `GINLINE-REQ-013` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::read_super_admin_list_unmigrated_returns_none` | custom sync | `GINLINE-REQ-016` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::read_super_admin_list_migrated_absent_returns_none` | custom sync | `GINLINE-REQ-016` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::read_super_admin_list_migrated_happy_path` | custom sync | `GINLINE-REQ-016` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::read_super_admin_list_malformed_bytes_surface_error` | custom sync | `GINLINE-REQ-016` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::read_group_metadata_unmigrated_returns_none` | custom sync | `GINLINE-REQ-016` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::read_group_metadata_missing_required_seeds_returns_none` | custom sync | `GINLINE-REQ-016` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::read_group_metadata_happy_path_non_dm` | custom sync | `GINLINE-REQ-016` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::read_group_metadata_dm_happy_path` | custom sync | `GINLINE-REQ-016` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::read_group_metadata_dm_wrong_cardinality_errors` | custom sync | `GINLINE-REQ-016` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::read_group_metadata_malformed_creator_errors` | custom sync | `GINLINE-REQ-016` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::read_group_membership_unmigrated_returns_none` | custom sync | `GINLINE-REQ-016` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::read_group_membership_happy_path_flattens_per_inbox` | custom sync | `GINLINE-REQ-016` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::read_group_membership_malformed_bytes_surface_error` | custom sync | `GINLINE-REQ-016` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::merge_with_malformed_registry_returns_valid_field` | custom sync | `GINLINE-REQ-019` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::merge_unmigrated_is_noop` | custom sync | `GINLINE-REQ-019` |
| `crates/xmtp_mls/src/groups/app_data/component_source.rs` | `groups::app_data::component_source::tests::merge_with_malformed_field_surfaces_error_not_silent_loss` | custom sync | `GINLINE-REQ-019` |
| `crates/xmtp_mls/src/groups/app_data/mod.rs` | `groups::app_data::tests::unmigrated_without_override_is_not_migrated` | built-in sync | `GINLINE-REQ-020` |
| `crates/xmtp_mls/src/groups/app_data/mod.rs` | `groups::app_data::tests::dict_without_registry_entry_is_not_migrated` | built-in sync | `GINLINE-REQ-020` |
| `crates/xmtp_mls/src/groups/app_data/mod.rs` | `groups::app_data::tests::dict_with_registry_entry_is_migrated` | built-in sync | `GINLINE-REQ-020` |
| `crates/xmtp_mls/src/groups/app_data/mod.rs` | `groups::app_data::tests::override_without_dict_entries_is_not_migrated` | Tokio async | `GINLINE-REQ-020` |
| `crates/xmtp_mls/src/groups/app_data/mod.rs` | `groups::app_data::tests::override_with_dict_entry_flips_migrated_in_tests` | Tokio async | `GINLINE-REQ-020` |
| `crates/xmtp_mls/src/groups/app_data/mod.rs` | `groups::app_data::tests::floor_above_own_version_fires` | built-in sync | `GINLINE-REQ-021` |
| `crates/xmtp_mls/src/groups/app_data/mod.rs` | `groups::app_data::tests::floor_at_or_below_own_version_does_not_fire` | built-in sync | `GINLINE-REQ-021` |
| `crates/xmtp_mls/src/groups/app_data/mod.rs` | `groups::app_data::tests::missing_floor_or_dict_does_not_fire` | built-in sync | `GINLINE-REQ-021` |
| `crates/xmtp_mls/src/groups/app_data/mod.rs` | `groups::app_data::tests::malformed_floor_is_lenient` | built-in sync | `GINLINE-REQ-021` |
| `crates/xmtp_mls/src/groups/app_data/mod.rs` | `groups::app_data::tests::load_registry_no_dict_returns_empty` | built-in sync | `GINLINE-REQ-022` |
| `crates/xmtp_mls/src/groups/app_data/mod.rs` | `groups::app_data::tests::load_registry_dict_without_entry_returns_empty` | built-in sync | `GINLINE-REQ-022` |
| `crates/xmtp_mls/src/groups/app_data/mod.rs` | `groups::app_data::tests::load_registry_with_valid_bytes_round_trips` | built-in sync | `GINLINE-REQ-022` |
| `crates/xmtp_mls/src/groups/app_data/mod.rs` | `groups::app_data::tests::load_registry_with_malformed_bytes_surfaces_error` | built-in sync | `GINLINE-REQ-022` |
| `crates/xmtp_mls/src/groups/app_data/migration.rs` | `groups::app_data::migration::tests::synthesize_initial_component_values_is_deterministic` | custom async; `unwrap_try` | `GINLINE-REQ-029` |
| `crates/xmtp_mls/src/groups/app_data/migration.rs` | `groups::app_data::migration::tests::synthesize_partitions_failed_installations_by_owner` | custom async; `unwrap_try` | `GINLINE-REQ-029` |
| `crates/xmtp_mls/src/groups/app_data/migration.rs` | `groups::app_data::migration::tests::synthesize_drops_unresolvable_failed_installations` | custom async; `unwrap_try` | `GINLINE-REQ-029` |
| `crates/xmtp_mls/src/groups/app_data/migration.rs` | `groups::app_data::migration::tests::synthesize_emits_expected_component_keys` | custom async; `unwrap_try` | `GINLINE-REQ-029` |
| `crates/xmtp_mls/src/groups/app_data/migration.rs` | `groups::app_data::migration::tests::synthesize_partitions_against_snapshotted_view_not_latest` | custom async; `unwrap_try` | `GINLINE-REQ-029` |
| `crates/xmtp_mls/src/groups/app_data/bootstrap_validator.rs` | `groups::app_data::bootstrap_validator::tests::happy_path_accepts_matching_bag` | built-in sync | `GINLINE-REQ-023` |
| `crates/xmtp_mls/src/groups/app_data/bootstrap_validator.rs` | `groups::app_data::bootstrap_validator::tests::missing_strict_component_surfaces_missing_seed` | built-in sync | `GINLINE-REQ-023` |
| `crates/xmtp_mls/src/groups/app_data/bootstrap_validator.rs` | `groups::app_data::bootstrap_validator::tests::byte_mismatch_surfaces_mismatch` | built-in sync | `GINLINE-REQ-023` |
| `crates/xmtp_mls/src/groups/app_data/bootstrap_validator.rs` | `groups::app_data::bootstrap_validator::tests::op_type_mismatch_surfaces_mismatch` | built-in sync | `GINLINE-REQ-023` |
| `crates/xmtp_mls/src/groups/app_data/bootstrap_validator.rs` | `groups::app_data::bootstrap_validator::tests::unexpected_component_surfaces_unexpected_proposal` | built-in sync | `GINLINE-REQ-023` |
| `crates/xmtp_mls/src/groups/app_data/bootstrap_validator.rs` | `groups::app_data::bootstrap_validator::tests::group_membership_sequence_id_mismatch_rejected` | built-in sync | `GINLINE-REQ-023` |
| `crates/xmtp_mls/src/groups/app_data/bootstrap_validator.rs` | `groups::app_data::bootstrap_validator::tests::group_membership_extra_member_rejected` | built-in sync | `GINLINE-REQ-023` |
| `crates/xmtp_mls/src/groups/app_data/bootstrap_validator.rs` | `groups::app_data::bootstrap_validator::tests::group_membership_missing_member_rejected` | built-in sync | `GINLINE-REQ-023` |
| `crates/xmtp_mls/src/groups/app_data/bootstrap_validator.rs` | `groups::app_data::bootstrap_validator::tests::group_membership_failed_installations_accepted_within_allowed_set` | built-in sync | `GINLINE-REQ-023` |
| `crates/xmtp_mls/src/groups/app_data/bootstrap_validator.rs` | `groups::app_data::bootstrap_validator::tests::group_membership_unauthorized_failed_installation_rejected` | built-in sync | `GINLINE-REQ-023` |
| `crates/xmtp_mls/src/groups/app_data/bootstrap_validator.rs` | `groups::app_data::bootstrap_validator::tests::component_registry_empty_round_trips` | built-in sync | `GINLINE-REQ-023` |
| `crates/xmtp_mls/src/groups/app_data/bootstrap_validator.rs` | `groups::app_data::bootstrap_validator::tests::component_registry_malformed_bytes_surface_decode` | built-in sync | `GINLINE-REQ-023` |
| `crates/xmtp_mls/src/groups/app_data/bootstrap_validator.rs` | `groups::app_data::bootstrap_validator::tests::group_membership_wrong_length_failed_installation_surfaces_decode` | built-in sync | `GINLINE-REQ-023` |
| `crates/xmtp_mls/src/groups/app_data/bootstrap_validator.rs` | `groups::app_data::bootstrap_validator::tests::group_membership_empty_both_sides_accepted` | built-in sync | `GINLINE-REQ-023` |
| `crates/xmtp_mls/src/groups/app_data/bootstrap_validator.rs` | `groups::app_data::bootstrap_validator::tests::group_membership_malformed_tlsmap_bytes_surfaces_decode` | built-in sync | `GINLINE-REQ-023` |
| `crates/xmtp_mls/src/groups/app_data/bootstrap_validator.rs` | `groups::app_data::bootstrap_validator::tests::allowed_bootstrap_proposal_predicate_rejects_membership_and_smuggled_types` | built-in sync | `GINLINE-REQ-023` |
| `crates/xmtp_mls/src/groups/app_data/bootstrap_validator.rs` | `groups::app_data::bootstrap_validator::tests::gce_extension_set_missing_required_capabilities_rejected` | built-in sync | `GINLINE-REQ-023` |
| `crates/xmtp_mls/src/groups/app_data/bootstrap_validator.rs` | `groups::app_data::bootstrap_validator::tests::gce_extension_set_with_required_capabilities_accepted` | built-in sync | `GINLINE-REQ-023` |
| `crates/xmtp_mls/src/groups/validated_commit.rs` | `groups::validated_commit::permission_on_receive_tests::non_admin_writing_super_admin_only_component_is_rejected` | custom sync; `unwrap_try` | `GINLINE-REQ-032` |
| `crates/xmtp_mls/src/groups/validated_commit.rs` | `groups::validated_commit::permission_on_receive_tests::plain_admin_writing_super_admin_only_component_is_rejected` | custom sync; `unwrap_try` | `GINLINE-REQ-032` |
| `crates/xmtp_mls/src/groups/validated_commit.rs` | `groups::validated_commit::permission_on_receive_tests::non_admin_writing_component_with_no_registry_entry_is_rejected` | custom sync; `unwrap_try` | `GINLINE-REQ-032` |
| `crates/xmtp_mls/src/groups/validated_commit.rs` | `groups::validated_commit::min_version_monotonicity_tests::first_set_with_no_prior_floor_is_allowed` | custom sync; `unwrap_try` | `GINLINE-REQ-033` |
| `crates/xmtp_mls/src/groups/validated_commit.rs` | `groups::validated_commit::min_version_monotonicity_tests::equal_version_is_allowed` | custom sync; `unwrap_try` | `GINLINE-REQ-033` |
| `crates/xmtp_mls/src/groups/validated_commit.rs` | `groups::validated_commit::min_version_monotonicity_tests::higher_version_is_allowed` | custom sync; `unwrap_try` | `GINLINE-REQ-033` |
| `crates/xmtp_mls/src/groups/validated_commit.rs` | `groups::validated_commit::min_version_monotonicity_tests::lower_version_is_rejected` | custom sync; `unwrap_try` | `GINLINE-REQ-033` |
| `crates/xmtp_mls/src/groups/validated_commit.rs` | `groups::validated_commit::min_version_monotonicity_tests::remove_with_prior_floor_is_rejected` | custom sync; `unwrap_try` | `GINLINE-REQ-033` |
| `crates/xmtp_mls/src/groups/validated_commit.rs` | `groups::validated_commit::min_version_monotonicity_tests::remove_with_no_prior_floor_is_allowed` | custom sync; `unwrap_try` | `GINLINE-REQ-033` |
| `crates/xmtp_mls/src/groups/validated_commit.rs` | `groups::validated_commit::min_version_monotonicity_tests::malformed_prior_skips_check` | custom sync; `unwrap_try` | `GINLINE-REQ-033` |
| `crates/xmtp_mls/src/groups/validated_commit.rs` | `groups::validated_commit::min_version_monotonicity_tests::malformed_new_value_surfaces_parse_error` | custom sync; `unwrap_try` | `GINLINE-REQ-033` |
| `crates/xmtp_mls/src/groups/validated_commit.rs` | `groups::validated_commit::min_version_monotonicity_tests::prerelease_ordering_matches_semver` | custom sync; `unwrap_try` | `GINLINE-REQ-033` |
| `crates/xmtp_mls/src/groups/validated_commit.rs` | `groups::validated_commit::min_version_monotonicity_tests::dev_prerelease_is_lower_than_release` | custom sync; `unwrap_try` | `GINLINE-REQ-033` |
| `crates/xmtp_mls/src/groups/commit_log_key.rs` | `groups::commit_log_key::tests::test_read_write_commit_log_key` | custom async; `unwrap_try` | `GINLINE-REQ-044` |
| `crates/xmtp_mls/src/groups/commit_log_key.rs` | `groups::commit_log_key::tests::test_verify_commit_log_signature` | custom async; `unwrap_try` | `GINLINE-REQ-045` |
| `crates/xmtp_mls/src/groups/commit_log_key.rs` | `groups::commit_log_key::tests::test_derive_consensus_public_key_with_valid_signature` | custom async; `unwrap_try` | `GINLINE-REQ-046` |
| `crates/xmtp_mls/src/groups/commit_log_key.rs` | `groups::commit_log_key::tests::test_derive_consensus_public_key_with_no_valid_signature` | custom async; `unwrap_try` | `GINLINE-REQ-046` |
| `crates/xmtp_mls/src/groups/commit_log_key.rs` | `groups::commit_log_key::tests::test_derive_consensus_public_key_with_invalid_signature` | custom async; `unwrap_try` | `GINLINE-REQ-046` |
| `crates/xmtp_mls/src/groups/commit_log_key.rs` | `groups::commit_log_key::tests::test_get_or_create_signing_key_uses_mutable_metadata` | custom async; `unwrap_try` | `GINLINE-REQ-047` |
| `crates/xmtp_mls/src/groups/commit_log_key.rs` | `groups::commit_log_key::tests::test_get_or_create_signing_key_ignores_non_matching_consensus` | custom async; `unwrap_try` | `GINLINE-REQ-047` |
| `crates/xmtp_mls/src/groups/commit_log_key.rs` | `groups::commit_log_key::tests::test_get_or_create_signing_key_uses_matching_stored_key` | custom async; `unwrap_try` | `GINLINE-REQ-047` |
| `crates/xmtp_mls/src/groups/commit_log_key.rs` | `groups::commit_log_key::tests::test_get_or_create_signing_key_uses_matching_mutable_metadata` | custom async; `unwrap_try` | `GINLINE-REQ-047` |
| `crates/xmtp_mls/src/groups/commit_log_key.rs` | `groups::commit_log_key::tests::test_get_or_create_signing_key_returns_none_with_consensus_no_matching_key` | custom async; `unwrap_try` | `GINLINE-REQ-047` |
| `crates/xmtp_mls/src/groups/error.rs` | `groups::error::tests::missing_sequence_id_is_retryable` | custom sync | `GINLINE-REQ-048` |
| `crates/xmtp_mls/src/groups/error.rs` | `groups::error::tests::failed_to_verify_installations_includes_hex_ids` | custom sync | `GINLINE-REQ-048` |
| `crates/xmtp_mls/src/groups/group_membership.rs` | `groups::group_membership::tests::test_equality_works` | custom sync | `GINLINE-REQ-043` |
| `crates/xmtp_mls/src/groups/group_membership.rs` | `groups::group_membership::tests::test_diff` | custom sync | `GINLINE-REQ-043` |
| `crates/xmtp_mls/src/groups/group_permissions.rs` | `groups::group_permissions::tests::test_allow_all` | custom sync | `GINLINE-REQ-034` |
| `crates/xmtp_mls/src/groups/group_permissions.rs` | `groups::group_permissions::tests::test_deny` | custom sync | `GINLINE-REQ-034` |
| `crates/xmtp_mls/src/groups/group_permissions.rs` | `groups::group_permissions::tests::test_actor_is_creator` | custom sync | `GINLINE-REQ-035` |
| `crates/xmtp_mls/src/groups/group_permissions.rs` | `groups::group_permissions::tests::test_and_condition` | custom sync | `GINLINE-REQ-036` |
| `crates/xmtp_mls/src/groups/group_permissions.rs` | `groups::group_permissions::tests::test_any_condition` | custom sync | `GINLINE-REQ-036` |
| `crates/xmtp_mls/src/groups/group_permissions.rs` | `groups::group_permissions::tests::test_serialize` | custom sync | `GINLINE-REQ-037` |
| `crates/xmtp_mls/src/groups/group_permissions.rs` | `groups::group_permissions::tests::test_update_group_name` | custom sync | `GINLINE-REQ-038` |
| `crates/xmtp_mls/src/groups/group_permissions.rs` | `groups::group_permissions::tests::test_preconfigured_policy` | custom sync | `GINLINE-REQ-039` |
| `crates/xmtp_mls/src/groups/group_permissions.rs` | `groups::group_permissions::tests::test_preconfigured_policy_equality_new_metadata` | custom sync | `GINLINE-REQ-039` |
| `crates/xmtp_mls/src/groups/group_permissions.rs` | `groups::group_permissions::tests::test_permission_update` | custom sync | `GINLINE-REQ-040` |
| `crates/xmtp_mls/src/groups/group_permissions.rs` | `groups::group_permissions::tests::test_evaluate_field_with_unknown_policy` | custom sync | `GINLINE-REQ-038`, `GINLINE-REQ-041` |
| `crates/xmtp_mls/src/groups/group_permissions.rs` | `groups::group_permissions::tests::test_dm_group_permissions` | custom sync | `GINLINE-REQ-042` |
| `crates/xmtp_mls/src/groups/intents.rs` | `groups::intents::app_data_update_intent_tests::round_trip_basic` | built-in sync | `GINLINE-REQ-052` |
| `crates/xmtp_mls/src/groups/intents.rs` | `groups::intents::app_data_update_intent_tests::empty_payload_round_trips` | built-in sync | `GINLINE-REQ-052` |
| `crates/xmtp_mls/src/groups/intents.rs` | `groups::intents::app_data_update_intent_tests::missing_version_variant_surfaces_error` | built-in sync | `GINLINE-REQ-052` |
| `crates/xmtp_mls/src/groups/intents.rs` | `groups::intents::app_data_update_intent_tests::component_id_overflow_surfaces_error` | built-in sync | `GINLINE-REQ-052` |
| `crates/xmtp_mls/src/groups/intents.rs` | `groups::intents::app_data_update_intent_tests::malformed_proto_bytes_surface_decode_error` | built-in sync | `GINLINE-REQ-052` |
| `crates/xmtp_mls/src/groups/intents.rs` | `groups::intents::tests::test_serialize_send_message` | cfg_attr: wasm-bindgen on wasm, built-in sync otherwise | `GINLINE-REQ-052` |
| `crates/xmtp_mls/src/groups/intents.rs` | `groups::intents::tests::test_serialize_update_membership` | cfg_attr: wasm-bindgen on wasm, Tokio otherwise | `GINLINE-REQ-052` |
| `crates/xmtp_mls/src/groups/intents.rs` | `groups::intents::tests::test_serialize_update_metadata` | custom async | `GINLINE-REQ-052` |
| `crates/xmtp_mls/src/groups/intents.rs` | `groups::intents::tests::test_serialize_readd_installations` | cfg_attr: wasm-bindgen on wasm, Tokio otherwise | `GINLINE-REQ-052` |
| `crates/xmtp_mls/src/groups/intents.rs` | `groups::intents::tests::test_key_rotation_before_first_message` | custom async | `SHARED-CONTENT-REQ-001` |
| `crates/xmtp_mls/src/groups/message_list.rs` | `groups::message_list::tests::test_exclude_content_types_with_custom_exclusions` | custom async | `GINLINE-REQ-058` |
| `crates/xmtp_mls/src/groups/message_list.rs` | `groups::message_list::tests::test_find_messages_no_reactions_or_replies` | custom async | `GINLINE-REQ-059` |
| `crates/xmtp_mls/src/groups/message_list.rs` | `groups::message_list::tests::test_find_messages_with_reactions` | custom async | `GINLINE-REQ-060` |
| `crates/xmtp_mls/src/groups/message_list.rs` | `groups::message_list::tests::test_find_messages_with_replies` | custom async | `GINLINE-REQ-061` |
| `crates/xmtp_mls/src/groups/message_list.rs` | `groups::message_list::tests::test_find_messages_invalid_reply_reference` | custom async | `GINLINE-REQ-061` |
| `crates/xmtp_mls/src/groups/message_list.rs` | `groups::message_list::tests::test_find_messages_missing_reply_reference` | custom async | `GINLINE-REQ-061` |
| `crates/xmtp_mls/src/groups/message_list.rs` | `groups::message_list::tests::test_find_messages_undecodable_messages` | custom async | `GINLINE-REQ-062` |
| `crates/xmtp_mls/src/groups/message_list.rs` | `groups::message_list::tests::test_find_messages_invalid_reactions` | custom async | `GINLINE-REQ-060` |
| `crates/xmtp_mls/src/groups/message_list.rs` | `groups::message_list::tests::test_hidden_message_types_are_filtered` | custom async | `GINLINE-REQ-058`, `GINLINE-REQ-060` |
| `crates/xmtp_mls/src/groups/message_list.rs` | `groups::message_list::tests::test_find_messages_chain_of_replies` | custom async | `GINLINE-REQ-061` |
| `crates/xmtp_mls/src/groups/message_list.rs` | `groups::message_list::tests::test_reply_with_custom_inner_content` | custom async | `GINLINE-REQ-061` |
| `crates/xmtp_mls/src/groups/mls_ext/mls_ext_welcome_pointee_encryption_aead_type.rs` | `groups::mls_ext::mls_ext_welcome_pointee_encryption_aead_type::tests::test_serialization` | custom sync | `GINLINE-REQ-082` |
| `crates/xmtp_mls/src/groups/mls_sync.rs` | `groups::mls_sync::tests::publish_intents_worst_case_scenario` | cfg_attr Tokio multi-thread with 10 workers; excluded on wasm family | `GINLINE-REQ-071` |
| `crates/xmtp_mls/src/groups/mls_sync.rs` | `groups::mls_sync::tests::hmac_keys_work_as_expected` | custom async | `GINLINE-REQ-072` |
| `crates/xmtp_mls/src/groups/mls_sync.rs` | `groups::mls_sync::tests::send_failures_for_published_intents_revert_to_to_publish` | built-in sync | `GINLINE-REQ-073` |
| `crates/xmtp_mls/src/groups/mls_sync.rs` | `groups::mls_sync::tests::test_process_delete_message_malformed_encoded_content` | custom async; `unwrap_try` | `GINLINE-REQ-074` |
| `crates/xmtp_mls/src/groups/mls_sync.rs` | `groups::mls_sync::tests::test_process_delete_message_malformed_inner_proto` | custom async; `unwrap_try` | `GINLINE-REQ-074` |
| `crates/xmtp_mls/src/groups/mls_sync.rs` | `groups::mls_sync::tests::test_process_delete_message_invalid_hex_message_id` | custom async; `unwrap_try` | `GINLINE-REQ-074` |
| `crates/xmtp_mls/src/groups/mls_sync.rs` | `groups::mls_sync::tests::process_message_with_app_data_error_commit_result_mapping` | built-in sync | `GINLINE-REQ-075` |
| `crates/xmtp_mls/src/groups/mls_sync/update_group_membership.rs` | `groups::mls_sync::update_group_membership::tests::applies_group_membership_intent` | rstest context fixture + custom async | `GINLINE-REQ-063` |
| `crates/xmtp_mls/src/groups/mls_sync/update_group_membership.rs` | `groups::mls_sync::update_group_membership::tests::strip_unverified_new_adds_removes_phantom_members` | custom sync | `GINLINE-REQ-064` |
| `crates/xmtp_mls/src/groups/oneshot.rs` | `groups::oneshot::tests::test_receive_oneshot_message_via_syncing` | Tokio async; module excluded on wasm | `GINLINE-REQ-065` |
| `crates/xmtp_mls/src/groups/oneshot.rs` | `groups::oneshot::tests::test_oneshot_groups_not_in_find_groups` | Tokio async; module excluded on wasm | `GINLINE-REQ-066` |
| `crates/xmtp_mls/src/groups/oneshot.rs` | `groups::oneshot::tests::test_oneshot_groups_not_in_stream_groups` | Tokio async; module excluded on wasm | `GINLINE-REQ-066` |
| `crates/xmtp_mls/src/groups/oneshot.rs` | `groups::oneshot::tests::test_syncing_and_streaming_oneshot_group_simultaneously` | Tokio async; module excluded on wasm | `GINLINE-REQ-067` |
| `crates/xmtp_mls/src/groups/send_message_opts.rs` | `groups::send_message_opts::tests::test_send_message_opts_builder` | built-in sync | `GINLINE-REQ-083` |
| `crates/xmtp_mls/src/groups/subscriptions.rs` | `groups::subscriptions::tests::test_subscribe_messages` | rstest + custom current-thread async; 10 s timeout | `SHARED-SYNC-REQ-005` |
| `crates/xmtp_mls/src/groups/subscriptions.rs` | `groups::subscriptions::tests::test_subscribe_multiple` | rstest + custom multi-thread async; 10 s timeout; ignored on wasm | `SHARED-SYNC-REQ-005` |
| `crates/xmtp_mls/src/groups/subscriptions.rs` | `groups::subscriptions::tests::test_subscribe_membership_changes` | rstest + custom async; 5 s timeout | `SHARED-SYNC-REQ-005` |
| `crates/xmtp_mls/src/groups/subscriptions.rs` | `groups::subscriptions::tests::test_process_streamed_group_message_v3` | rstest context fixture + custom multi-thread/1-worker async; 5 s timeout | `GINLINE-REQ-070` |
| `crates/xmtp_mls/src/groups/subscriptions.rs` | `groups::subscriptions::tests::test_process_streamed_group_message_d14n` | rstest context fixture + custom multi-thread/1-worker async; 5 s timeout | `GINLINE-REQ-070` |
| `crates/xmtp_mls/src/groups/summary.rs` | `groups::summary::extend_tests::extend_preserves_first_other_cause` | custom sync | `GINLINE-REQ-049` |
| `crates/xmtp_mls/src/groups/summary.rs` | `groups::summary::extend_tests::extend_takes_other_when_none_yet` | custom sync | `GINLINE-REQ-049` |
| `crates/xmtp_mls/src/groups/summary.rs` | `groups::summary::tests::clean_summary_is_not_errored` | custom sync | `GINLINE-REQ-049` |
| `crates/xmtp_mls/src/groups/summary.rs` | `groups::summary::tests::publish_only_error_is_errored` | custom sync | `GINLINE-REQ-049` |
| `crates/xmtp_mls/src/groups/summary.rs` | `groups::summary::tests::post_commit_only_error_is_errored` | custom sync | `GINLINE-REQ-049` |
| `crates/xmtp_mls/src/groups/summary.rs` | `groups::summary::tests::other_error_is_errored_and_is_source` | custom sync | `GINLINE-REQ-049` |
| `crates/xmtp_mls/src/groups/summary.rs` | `groups::summary::tests::per_message_failures_do_not_flip_errored` | custom sync | `GINLINE-REQ-049` |
| `crates/xmtp_mls/src/groups/summary.rs` | `groups::summary::tests::source_prefers_other_over_per_message_error` | custom sync | `GINLINE-REQ-049` |
| `crates/xmtp_mls/src/groups/welcome_sync.rs` | `groups::welcome_sync::tests::happy_path` | rstest context fixture + custom async | `GINLINE-REQ-076` |
| `crates/xmtp_mls/src/groups/welcome_sync.rs` | `groups::welcome_sync::tests::increments_cursor_on_non_retryable_in_tx` | rstest context fixture + custom async | `GINLINE-REQ-077` |
| `crates/xmtp_mls/src/groups/welcome_sync.rs` | `groups::welcome_sync::tests::increments_cursor_on_non_retryable_during_validation` | rstest context fixture + custom async | `GINLINE-REQ-077` |
| `crates/xmtp_mls/src/groups/welcome_sync.rs` | `groups::welcome_sync::tests::increments_message_cursor_from_welcome_metadata` | rstest context fixture + custom async | `GINLINE-REQ-079` |
| `crates/xmtp_mls/src/groups/welcome_sync.rs` | `groups::welcome_sync::tests::does_not_increment` | rstest + custom async; cases: non-retryable/flag false, retryable/flag true | `GINLINE-REQ-077`, `GINLINE-REQ-078` |
| `crates/xmtp_mls/src/groups/welcome_sync.rs` | `groups::welcome_sync::tests::later_welcome_must_not_advance_cursor_past_retryable_failure` | rstest context fixture + custom async | `GINLINE-REQ-078` |
| `crates/xmtp_mls/src/groups/welcome_sync.rs` | `groups::welcome_sync::tests::filter_groups_with_new_messages_basic_behavior` | custom sync | `GINLINE-REQ-080` |
| `crates/xmtp_mls/src/groups/welcome_sync.rs` | `groups::welcome_sync::tests::filter_groups_includes_never_synced_and_excludes_up_to_date` | custom sync | `GINLINE-REQ-080` |
| `crates/xmtp_mls/src/groups/welcome_sync.rs` | `groups::welcome_sync::tests::filter_groups_handles_multiple_originators` | custom sync | `GINLINE-REQ-080` |
| `crates/xmtp_mls/src/groups/welcome_sync.rs` | `groups::welcome_sync::tests::filter_groups_treats_unknown_originator_as_new` | custom sync | `GINLINE-REQ-080` |
| `crates/xmtp_mls/src/groups/welcome_sync.rs` | `groups::welcome_sync::tests::filter_groups_returns_empty_when_no_updates` | native-only rstest + custom sync; cases: empty maps, equal cursors | `GINLINE-REQ-080` |
| `crates/xmtp_mls/src/groups/welcome_sync.rs` | `groups::welcome_sync::tests::filter_groups_comprehensive_mixed_states` | custom sync | `GINLINE-REQ-080` |
| `crates/xmtp_mls/src/groups/welcomes/xmtp_welcome.rs` | `groups::welcomes::xmtp_welcome::tests::welcome_builds_with_default_events` | rstest context fixture + custom async | `GINLINE-REQ-081` |
| `crates/xmtp_mls/src/messages/tests/test_deletion_validation.rs` | `messages::tests::test_deletion_validation::test_valid_deletion_by_sender` | built-in sync | `GINLINE-REQ-084`, `SHARED-CONTENT-REQ-005` |
| `crates/xmtp_mls/src/messages/tests/test_deletion_validation.rs` | `messages::tests::test_deletion_validation::test_valid_deletion_by_super_admin` | built-in sync | `GINLINE-REQ-084`, `SHARED-CONTENT-REQ-005` |
| `crates/xmtp_mls/src/messages/tests/test_deletion_validation.rs` | `messages::tests::test_deletion_validation::test_invalid_deletion_unauthorized` | built-in sync | `GINLINE-REQ-084` |
| `crates/xmtp_mls/src/messages/tests/test_deletion_validation.rs` | `messages::tests::test_deletion_validation::test_invalid_deletion_message_id_mismatch` | built-in sync | `SHARED-CONTENT-REQ-004` |
| `crates/xmtp_mls/src/messages/tests/test_deletion_validation.rs` | `messages::tests::test_deletion_validation::test_invalid_deletion_cross_group_deletion_group_mismatch` | built-in sync | `SHARED-CONTENT-REQ-004` |
| `crates/xmtp_mls/src/messages/tests/test_deletion_validation.rs` | `messages::tests::test_deletion_validation::test_invalid_deletion_message_group_mismatch` | built-in sync | `SHARED-CONTENT-REQ-004` |
| `crates/xmtp_mls/src/messages/tests/test_deletion_validation.rs` | `messages::tests::test_deletion_validation::test_invalid_deletion_non_deletable_content_type` | built-in sync | `SHARED-CONTENT-REQ-005` |
| `crates/xmtp_mls/src/messages/tests/test_deletion_validation.rs` | `messages::tests::test_deletion_validation::test_invalid_deletion_non_deletable_message_kind` | built-in sync | `SHARED-CONTENT-REQ-005` |
| `crates/xmtp_mls/src/messages/tests/test_deletion_validation.rs` | `messages::tests::test_deletion_validation::test_invalid_deletion_delete_message_content_type` | built-in sync | `SHARED-CONTENT-REQ-005` |
| `crates/xmtp_mls/src/messages/tests/test_deletion_validation.rs` | `messages::tests::test_deletion_validation::test_invalid_deletion_read_receipt_not_deletable` | built-in sync | `SHARED-CONTENT-REQ-005` |
| `crates/xmtp_mls/src/messages/tests/test_deletion_validation.rs` | `messages::tests::test_deletion_validation::test_invalid_deletion_reaction_not_deletable` | built-in sync | `SHARED-CONTENT-REQ-005` |
| `crates/xmtp_mls/src/messages/tests/test_deletion_validation.rs` | `messages::tests::test_deletion_validation::test_valid_deletion_markdown_content` | built-in sync | `SHARED-CONTENT-REQ-005` |
| `crates/xmtp_mls/src/messages/tests/test_deletion_validation.rs` | `messages::tests::test_deletion_validation::test_valid_deletion_reply_content` | built-in sync | `SHARED-CONTENT-REQ-005` |
| `crates/xmtp_mls/src/messages/tests/test_deletion_validation.rs` | `messages::tests::test_deletion_validation::test_valid_deletion_attachment_content` | built-in sync | `SHARED-CONTENT-REQ-005` |
| `crates/xmtp_mls/src/messages/tests/test_deletion_validation.rs` | `messages::tests::test_deletion_validation::test_valid_deletion_remote_attachment_content` | built-in sync | `SHARED-CONTENT-REQ-005` |
