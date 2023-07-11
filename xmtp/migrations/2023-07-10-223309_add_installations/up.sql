CREATE TABLE installations (
    installation_id TEXT PRIMARY KEY NOT NULL,
    user_address TEXT NOT NULL,
    first_seen BIGINT NOT NULL,
    contact BLOB NOT NULL,
    expires_at BIGINT,
    FOREIGN KEY(user_address) REFERENCES users(user_address)
);