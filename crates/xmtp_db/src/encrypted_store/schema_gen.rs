// @generated automatically by Diesel CLI.

diesel::table! {
    association_state (inbox_id, sequence_id) {
        inbox_id -> Text,
        sequence_id -> BigInt,
        state -> Binary,
    }
}

diesel::table! {
    consent_records (entity_type, entity) {
        entity_type -> Integer,
        state -> Integer,
        entity -> Text,
        consented_at_ns -> BigInt,
    }
}

diesel::table! {
    d14n_migration_cutover (id) {
        id -> Integer,
        cutover_ns -> BigInt,
        last_checked_ns -> BigInt,
        has_migrated -> Bool,
    }
}

diesel::table! {
    group_intents (id) {
        id -> Integer,
        kind -> Integer,
        group_id -> Binary,
        data -> Binary,
        state -> Integer,
        payload_hash -> Nullable<Binary>,
        post_commit_data -> Nullable<Binary>,
        publish_attempts -> Integer,
        staged_commit -> Nullable<Binary>,
        published_in_epoch -> Nullable<BigInt>,
        should_push -> Bool,
        sequence_id -> Nullable<BigInt>,
        originator_id -> Nullable<BigInt>,
    }
}

diesel::table! {
    group_messages (id) {
        id -> Binary,
        group_id -> Binary,
        decrypted_message_bytes -> Binary,
        sent_at_ns -> BigInt,
        kind -> Integer,
        sender_installation_id -> Binary,
        sender_inbox_id -> Text,
        delivery_status -> Integer,
        content_type -> Integer,
        version_major -> Integer,
        version_minor -> Integer,
        authority_id -> Text,
        reference_id -> Nullable<Binary>,
        originator_id -> BigInt,
        sequence_id -> BigInt,
        inserted_at_ns -> BigInt,
        expire_at_ns -> Nullable<BigInt>,
        should_push -> Bool,
    }
}

diesel::table! {
    groups (id) {
        id -> Binary,
        created_at_ns -> BigInt,
        membership_state -> Integer,
        installations_last_checked -> BigInt,
        added_by_inbox_id -> Text,
        sequence_id -> Nullable<BigInt>,
        rotated_at_ns -> BigInt,
        conversation_type -> Integer,
        dm_id -> Nullable<Text>,
        last_message_ns -> Nullable<BigInt>,
        message_disappear_from_ns -> Nullable<BigInt>,
        message_disappear_in_ns -> Nullable<BigInt>,
        paused_for_version -> Nullable<Text>,
        maybe_forked -> Bool,
        fork_details -> Text,
        originator_id -> Nullable<BigInt>,
        should_publish_commit_log -> Bool,
        commit_log_public_key -> Nullable<Binary>,
        is_commit_log_forked -> Nullable<Bool>,
        has_pending_leave_request -> Nullable<Bool>,
    }
}

diesel::table! {
    icebox (originator_id, sequence_id) {
        originator_id -> BigInt,
        sequence_id -> BigInt,
        group_id -> Binary,
        envelope_payload -> Binary,
    }
}

diesel::table! {
    icebox_dependencies (envelope_originator_id, envelope_sequence_id, dependency_originator_id, dependency_sequence_id) {
        envelope_originator_id -> BigInt,
        envelope_sequence_id -> BigInt,
        dependency_originator_id -> BigInt,
        dependency_sequence_id -> BigInt,
    }
}

diesel::table! {
    identity (rowid) {
        inbox_id -> Text,
        installation_keys -> Binary,
        credential_bytes -> Binary,
        rowid -> Nullable<Integer>,
        next_key_package_rotation_ns -> Nullable<BigInt>,
    }
}

diesel::table! {
    identity_cache (identity, identity_kind) {
        inbox_id -> Text,
        identity -> Text,
        identity_kind -> Integer,
    }
}

diesel::table! {
    identity_updates (inbox_id, sequence_id) {
        inbox_id -> Text,
        sequence_id -> BigInt,
        server_timestamp_ns -> BigInt,
        payload -> Binary,
        originator_id -> Integer,
    }
}

diesel::table! {
    key_package_history (id) {
        id -> Integer,
        key_package_hash_ref -> Binary,
        created_at_ns -> BigInt,
        delete_at_ns -> Nullable<BigInt>,
        post_quantum_public_key -> Nullable<Binary>,
    }
}

diesel::table! {
    local_commit_log (rowid) {
        rowid -> Integer,
        group_id -> Binary,
        commit_sequence_id -> BigInt,
        last_epoch_authenticator -> Binary,
        commit_result -> Integer,
        applied_epoch_number -> BigInt,
        applied_epoch_authenticator -> Binary,
        error_message -> Nullable<Text>,
        sender_inbox_id -> Nullable<Text>,
        sender_installation_id -> Nullable<Binary>,
        commit_type -> Nullable<Text>,
    }
}

diesel::table! {
    message_deletions (id) {
        id -> Binary,
        group_id -> Binary,
        deleted_message_id -> Binary,
        deleted_by_inbox_id -> Text,
        is_super_admin_deletion -> Bool,
        deleted_at_ns -> BigInt,
    }
}

diesel::table! {
    openmls_key_store (key_bytes) {
        key_bytes -> Binary,
        value_bytes -> Binary,
    }
}

diesel::table! {
    openmls_key_value (version, key_bytes) {
        version -> Integer,
        key_bytes -> Binary,
        value_bytes -> Binary,
    }
}

diesel::table! {
    pending_remove (group_id, inbox_id) {
        group_id -> Binary,
        inbox_id -> Text,
        message_id -> Binary,
    }
}

diesel::table! {
    processed_device_sync_messages (message_id) {
        message_id -> Binary,
        attempts -> Integer,
        state -> Integer,
    }
}

diesel::table! {
    readd_status (group_id, installation_id) {
        group_id -> Binary,
        installation_id -> Binary,
        requested_at_sequence_id -> Nullable<BigInt>,
        responded_at_sequence_id -> Nullable<BigInt>,
    }
}

diesel::table! {
    refresh_state (entity_id, entity_kind, originator_id) {
        entity_id -> Binary,
        entity_kind -> Integer,
        sequence_id -> BigInt,
        originator_id -> Integer,
    }
}

diesel::table! {
    remote_commit_log (rowid) {
        rowid -> Integer,
        log_sequence_id -> BigInt,
        group_id -> Binary,
        commit_sequence_id -> BigInt,
        commit_result -> Integer,
        applied_epoch_number -> BigInt,
        applied_epoch_authenticator -> Binary,
    }
}

diesel::table! {
    tasks (id) {
        id -> Integer,
        originating_message_sequence_id -> BigInt,
        originating_message_originator_id -> Integer,
        created_at_ns -> BigInt,
        expires_at_ns -> BigInt,
        attempts -> Integer,
        max_attempts -> Integer,
        last_attempted_at_ns -> BigInt,
        backoff_scaling_factor -> Float,
        max_backoff_duration_ns -> BigInt,
        initial_backoff_duration_ns -> BigInt,
        next_attempt_at_ns -> BigInt,
        data_hash -> Binary,
        data -> Binary,
    }
}

diesel::table! {
    user_preferences (id) {
        id -> Integer,
        hmac_key -> Nullable<Binary>,
        hmac_key_cycled_at_ns -> Nullable<BigInt>,
        dm_group_updates_migrated -> Bool,
    }
}

diesel::joinable!(group_intents -> groups (group_id));
diesel::joinable!(group_messages -> groups (group_id));
diesel::joinable!(icebox -> groups (group_id));
diesel::joinable!(message_deletions -> group_messages (id));

diesel::allow_tables_to_appear_in_same_query!(
    association_state,
    consent_records,
    d14n_migration_cutover,
    group_intents,
    group_messages,
    groups,
    icebox,
    icebox_dependencies,
    identity,
    identity_cache,
    identity_updates,
    key_package_history,
    local_commit_log,
    message_deletions,
    openmls_key_store,
    openmls_key_value,
    pending_remove,
    processed_device_sync_messages,
    readd_status,
    refresh_state,
    remote_commit_log,
    tasks,
    user_preferences,
);
