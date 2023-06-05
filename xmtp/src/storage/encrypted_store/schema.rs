// @generated automatically by Diesel CLI.

diesel::table! {
    accounts (id) {
        id -> Integer,
        created_at -> BigInt,
        serialized_key -> Text,
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

diesel::allow_tables_to_appear_in_same_query!(accounts, messages,);
