CREATE TABLE client_events (
    created_at_ns BIGINT NOT NULL,
    group_id BLOB,
    details BLOB NOT NULL
);
