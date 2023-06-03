// @generated automatically by Diesel CLI.

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
        peer_address -> Text,
        peer_installation_id -> Text,
        vmac_session_data -> Binary,
    }
}
