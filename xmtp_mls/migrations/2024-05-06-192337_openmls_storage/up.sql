CREATE TABLE IF NOT EXISTS openmls_key_value (
    version INT NOT NULL,
    key_bytes BLOB NOT NULL,
    value_bytes BLOB NOT NULL,
    PRIMARY KEY (version, key_bytes)
);

CREATE TABLE IF NOT EXISTS openmls_arrays (
    version INT NOT NULL,
    key_bytes BLOB NOT NULL,
    value_bytes BLOB NOT NULL,
    sequence_id SERIAL NOT NULL,
    PRIMARY KEY (version, key_bytes, sequence_id)
);

