CREATE TABLE outbound_payloads (
    created_at_ns BIGINT PRIMARY KEY NOT NULL,
    content_topic TEXT NOT NULL,
    payload BLOB NOT NULL,
    outbound_payload_state INTEGER NOT NULL
);