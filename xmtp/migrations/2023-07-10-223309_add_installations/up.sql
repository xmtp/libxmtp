CREATE TABLE installations (
    installation_id TEXT PRIMARY KEY NOT NULL,
    user_address TEXT NOT NULL,
    first_seen_ns BIGINT NOT NULL,
    contact BLOB NOT NULL,
    contact_hash TEXT NOT NULL,
    expires_at_ns BIGINT,
    FOREIGN KEY(user_address) REFERENCES users(user_address)
);

ALTER TABLE sessions
  ADD user_address TEXT NOT NULL;