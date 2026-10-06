CREATE TABLE IF NOT EXISTS openmls_key_value (
    version INT NOT NULL,
    key_bytes BLOB NOT NULL,
    value_bytes BLOB NOT NULL,
    PRIMARY KEY (version, key_bytes)
);