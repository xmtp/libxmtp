CREATE TABLE identity_inbox (
    "inbox_id" TEXT NOT NULL,
    "installation_keys" BLOB NOT NULL,
    "credential_bytes" BLOB NOT NULL,
    rowid INTEGER PRIMARY KEY CHECK (rowid = 1)    -- There can only be one identity
);
