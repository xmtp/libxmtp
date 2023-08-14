CREATE TABLE IF NOT EXISTS sessions (
    peer_installation_id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    created_at BIGINT NOT NULL,
    vmac_session_data BLOB NOT NULL
)
