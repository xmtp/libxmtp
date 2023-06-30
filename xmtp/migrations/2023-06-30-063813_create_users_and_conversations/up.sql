CREATE TABLE users (
    user_address TEXT PRIMARY KEY NOT NULL,
    created_at BIGINT NOT NULL,
    last_refreshed BIGINT NOT NULL
);
CREATE TABLE conversations (
  convo_id TEXT PRIMARY KEY NOT NULL,
  peer_address TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  convo_state INTEGER NOT NULL,
  FOREIGN KEY(peer_address) REFERENCES users(user_address)
);