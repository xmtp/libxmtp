// @generated automatically by Diesel CLI.

diesel::table! {
    association_state (inbox_id, sequence_id) {
        inbox_id -> Text,
        sequence_id -> BigInt,
        state -> Binary,
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
    }
}

diesel::table! {
    groups (id) {
        id -> Binary,
        created_at_ns -> BigInt,
        membership_state -> Integer,
        installations_last_checked -> BigInt,
        purpose -> Integer,
        added_by_inbox_id -> Text,
        welcome_id -> Nullable<BigInt>,
        rotated_at_ns -> BigInt,
    }
}

diesel::table! {
    identity (rowid) {
        inbox_id -> Text,
        installation_keys -> Binary,
        credential_bytes -> Binary,
        rowid -> Nullable<Integer>,
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
    refresh_state (entity_id, entity_kind) {
        entity_id -> Binary,
        entity_kind -> Integer,
        cursor -> BigInt,
    }
}

diesel::joinable!(group_intents -> groups (group_id));
diesel::joinable!(group_messages -> groups (group_id));

diesel::allow_tables_to_appear_in_same_query!(
    association_state,
    group_intents,
    group_messages,
    groups,
    identity,
    identity_updates,
    openmls_key_store,
    openmls_key_value,
    refresh_state,
);
