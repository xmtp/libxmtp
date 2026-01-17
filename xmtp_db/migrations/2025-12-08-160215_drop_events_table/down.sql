CREATE TABLE events (
    created_at_ns BIGINT NOT NULL,
    group_id BLOB,
    event TEXT NOT NULL,
    details BLOB,
    level INTEGER NOT NULL DEFAULT 0,
    icon TEXT
);

CREATE INDEX idx_event_name ON events (event);
