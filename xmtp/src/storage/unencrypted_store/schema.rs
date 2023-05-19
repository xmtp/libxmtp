// @generated automatically by Diesel CLI.

diesel::table! {
    messages (id) {
        id -> Integer,
        created_at -> BigInt,
        convoid -> Text,
        addr_from -> Text,
        content -> Text,
    }
}
