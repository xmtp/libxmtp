CREATE TABLE inbound_messages (
    id TEXT PRIMARY KEY NOT NULL,
    sent_at_ns BIGINT NOT NULL,
    payload BLOB NOT NULL,
    topic TEXT NOT NULL,
    status SMALLINT NOT NULL
);