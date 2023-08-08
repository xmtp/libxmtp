CREATE TABLE outbound_payloads (
    msg_id TEXT NOT NULL,
    created_at_ns BIGINT KEY NOT NULL,
    content_topic TEXT NOT NULL,
    payload BLOB NOT NULL,
    outbound_payload_state INTEGER NOT NULL
);