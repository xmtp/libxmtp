CREATE TABLE client_events (
    id INTEGER PRIMARY KEY NOT NULL,
    created_at_ns BIGINT NOT NULL,
    event INTEGER NOT NULL,
    details BLOB NOT NULL
);
