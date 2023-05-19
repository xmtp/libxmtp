// @generated automatically by Diesel CLI.

diesel::table! {
    messages (id) {
        id -> Integer,
        created_at -> Float,
        convoid -> Text,
        addr_from -> Text,
        content -> Text,
    }
}
