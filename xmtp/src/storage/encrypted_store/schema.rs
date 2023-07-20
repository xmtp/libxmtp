// @generated automatically by Diesel CLI.

diesel::table! {
    accounts (id) {
        id -> Integer,
        created_at -> BigInt,
        serialized_key -> Binary,
    }
}

diesel::table! {
    conversations (convo_id) {
        convo_id -> Text,
        peer_address -> Text,
        created_at -> BigInt,
        convo_state -> Integer,
    }
}

diesel::table! {
    installations (installation_id) {
        installation_id -> Text,
        user_address -> Text,
        first_seen_ns -> BigInt,
        contact -> Binary,
        contact_hash -> Text,
        expires_at_ns -> Nullable<BigInt>,
    }
}

diesel::table! {
    messages (id) {
        id -> Integer,
        created_at -> BigInt,
        convo_id -> Text,
        addr_from -> Text,
        content -> Binary,
    }
}

diesel::table! {
    sessions (session_id) {
        session_id -> Text,
        created_at -> BigInt,
        peer_installation_id -> Text,
        vmac_session_data -> Binary,
        user_address -> Text,
    }
}

diesel::table! {
    users (user_address) {
        user_address -> Text,
        created_at -> BigInt,
        last_refreshed -> BigInt,
    }
}

diesel::joinable!(conversations -> users (peer_address));
diesel::joinable!(installations -> users (user_address));

diesel::allow_tables_to_appear_in_same_query!(
    accounts,
    conversations,
    installations,
    messages,
    sessions,
    users,
);
