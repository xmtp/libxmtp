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
    messages (id) {
        id -> Integer,
        created_at -> BigInt,
        convo_id -> Text,
        addr_from -> Text,
        content -> Binary,
        state -> Integer,
    }
}

diesel::table! {
    sessions (session_id) {
        session_id -> Text,
        created_at -> BigInt,
        peer_installation_id -> Text,
        vmac_session_data -> Binary,
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

diesel::allow_tables_to_appear_in_same_query!(
    accounts,
    conversations,
    messages,
    sessions,
    users,
);
