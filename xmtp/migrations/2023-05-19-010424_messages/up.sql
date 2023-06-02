CREATE TABLE IF NOT EXISTS messages (
    id INTEGER PRIMARY KEY NOT NULL,
    created_at INTEGER NOT NULL,
    convo_id TEXT NOT NULL,
    addr_from TEXT NOT NULL,
    content BLOB NOT NULL
);

CREATE TABLE IF NOT EXISTS accounts (
    id INTEGER PRIMARY KEY NOT NULL,
    serialized_key TEXT NOT NULL
);
