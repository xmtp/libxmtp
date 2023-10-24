// @generated automatically by Diesel CLI.

diesel::table! {
    openmls_key_store (key_bytes) {
        key_bytes -> Binary,
        value_bytes -> Binary,
    }
}
