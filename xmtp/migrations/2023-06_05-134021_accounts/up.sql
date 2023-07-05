CREATE TABLE IF NOT EXISTS accounts (
    id INTEGER PRIMARY KEY NOT NULL,
    created_at BIGINT NOT NULL,
    serialized_key BLOB NOT NULL
);
