// @generated automatically by Diesel CLI.

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

diesel::allow_tables_to_appear_in_same_query!(
    identity,
    openmls_key_store,
);
