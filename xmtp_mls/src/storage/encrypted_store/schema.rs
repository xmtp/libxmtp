// @generated automatically by Diesel CLI.

diesel::table! {
    group_intents (id) {
        id -> Integer,
        kind -> Integer,
        group_id -> Binary,
        data -> Binary,
        state -> Integer,
        payload_hash -> Nullable<Binary>,
        post_commit_data -> Nullable<Binary>,
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
        sender_account_address -> Text,
    }
}

diesel::table! {
    groups (id) {
        id -> Binary,
        created_at_ns -> BigInt,
        membership_state -> Integer,
    }
}

diesel::table! {
    identity (rowid) {
        account_address -> Text,
        installation_keys -> Binary,
        credential_bytes -> Binary,
        rowid -> Nullable<Integer>,
    }
}

diesel::table! {
    openmls_key_store (key_bytes) {
        key_bytes -> Binary,
        value_bytes -> Binary,
    }
}

diesel::table! {
    topic_refresh_state (topic) {
        topic -> Text,
        last_message_timestamp_ns -> BigInt,
    }
}

diesel::joinable!(group_intents -> groups (group_id));
diesel::joinable!(group_messages -> groups (group_id));

diesel::allow_tables_to_appear_in_same_query!(
    group_intents,
    group_messages,
    groups,
    identity,
    openmls_key_store,
    topic_refresh_state,
);
