CREATE TABLE IF NOT EXISTS openmls_key_store (
    key_bytes BLOB PRIMARY KEY NOT NULL,
    value_bytes BLOB NOT NULL
);
