CREATE TABLE inbound_invites (
    id TEXT PRIMARY KEY NOT NULL,
    sent_at_ns BIGINT NOT NULL,
    payload BLOB NOT NULL,
    topic TEXT NOT NULL,
    status SMALLINT NOT NULL
);
INSERT INTO refresh_jobs VALUES ('invite', 0);
