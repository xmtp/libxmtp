CREATE TABLE IF NOT EXISTS messages (
    id INTEGER PRIMARY KEY NOT NULL,
    created_at REAL NOT NULL,
    convoid TEXT NOT NULL,
    addr_from TEXT NOT NULL,
    content TEXT NOT NULL
)