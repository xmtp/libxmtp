// @generated automatically by Diesel CLI.

use super::schema::conversation_list;

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
    events (rowid) {
        rowid -> Integer,
        created_at_ns -> BigInt,
        group_id -> Nullable<Binary>,
        event -> Text,
        details -> Nullable<Binary>,
        level -> Integer,
        icon -> Nullable<Text>,
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
        version_minor -> Integer,
        version_major -> Integer,
        authority_id -> Text,
        reference_id -> Nullable<Binary>,
        sequence_id -> Nullable<BigInt>,
        originator_id -> Nullable<BigInt>,
    }
}

diesel::table! {
    groups (id) {
        id -> Binary,
        created_at_ns -> BigInt,
        membership_state -> Integer,
        installations_last_checked -> BigInt,
        added_by_inbox_id -> Text,
        welcome_id -> Nullable<BigInt>,
        rotated_at_ns -> BigInt,
        conversation_type -> Integer,
        dm_id -> Nullable<Text>,
        last_message_ns -> Nullable<BigInt>,
        message_disappear_from_ns -> Nullable<BigInt>,
        message_disappear_in_ns -> Nullable<BigInt>,
        paused_for_version -> Nullable<Text>,
        maybe_forked -> Bool,
        fork_details -> Text,
        sequence_id -> Nullable<BigInt>,
        originator_id -> Nullable<BigInt>,
    }
}

diesel::table! {
    icebox (sequence_id, originator_id) {
        sequence_id -> BigInt,
        originator_id -> BigInt,
        depending_sequence_id -> Nullable<BigInt>,
        depending_originator_id -> Nullable<BigInt>,
        envelope_payload -> Binary,
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
    }
}

diesel::table! {
    key_package_history (id) {
        id -> Integer,
        key_package_hash_ref -> Binary,
        created_at_ns -> BigInt,
        delete_at_ns -> Nullable<BigInt>,
    }
}

diesel::table! {
    local_commit_log (sequence_id) {
        sequence_id -> BigInt,
        group_id -> Binary,
        epoch_authenticator -> Binary,
        result -> Integer,
        epoch_number -> BigInt,
        sender_inbox_id -> Nullable<Text>,
        sender_installation_id -> Nullable<Binary>,
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
    processed_device_sync_messages (message_id) {
        message_id -> Binary,
    }
}

diesel::table! {
    refresh_state (entity_id, entity_kind) {
        entity_id -> Binary,
        entity_kind -> Integer,
        cursor -> BigInt,
    }
}

diesel::table! {
    remote_commit_log (sequence_id) {
        sequence_id -> BigInt,
        group_id -> Binary,
        last_epoch_authenticator -> Nullable<Binary>,
        epoch_authenticator -> Binary,
        result -> Integer,
        epoch_number -> Nullable<BigInt>,
    }
}

diesel::table! {
    user_preferences (id) {
        id -> Integer,
        hmac_key -> Nullable<Binary>,
        hmac_key_cycled_at_ns -> Nullable<BigInt>,
    }
}

diesel::joinable!(group_intents -> groups (group_id));
diesel::joinable!(group_messages -> groups (group_id));

diesel::allow_tables_to_appear_in_same_query!(
    association_state,
    consent_records,
    events,
    group_intents,
    group_messages,
    groups,
    icebox,
    identity,
    identity_cache,
    identity_updates,
    key_package_history,
    local_commit_log,
    openmls_key_store,
    openmls_key_value,
    processed_device_sync_messages,
    refresh_state,
    remote_commit_log,
    user_preferences,
    conversation_list
);
