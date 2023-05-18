// TODO: Automate the generation form Diesel-CLI

diesel::table! {
    channels (id) {
        id -> Integer,
        channel_type -> Text,
        created_at -> Float,
        display_name -> Text,
        members -> Text,
    }
}

diesel::table! {
    messages (id) {
        id -> Integer,
        created_at -> Float,
        convoid -> Text,
        addr_from -> Text,
        content -> Text,
    }
}

diesel::allow_tables_to_appear_in_same_query!(channels, messages,);
