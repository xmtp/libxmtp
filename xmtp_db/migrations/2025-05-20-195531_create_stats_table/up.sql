CREATE TABLE events (
    created_at_ns BIGINT NOT NULL,
    group_id BLOB,
    event String NOT NULL,
    details BLOB
);

CREATE INDEX idx_event_name ON events (event);
