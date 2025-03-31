ALTER TABLE user_preferences
RENAME TO user_preferences_old;

CREATE TABLE user_preferences (
    id INTEGER PRIMARY KEY NOT NULL DEFAULT 0 CHECK (id = 0),
    hmac_key BLOB,
    sync_cursor TEXT
);

INSERT INTO
    user_preferences (hmac_key)
SELECT
    hmac_key
FROM
    user_preferences_old
ORDER BY
    id DESC
LIMIT
    1;

DROP TABLE user_preferences_old;

-- Add an inserted_at_ns to group_messages
ALTER TABLE group_messages
ADD COLUMN inserted_at_ns BIGINT DEFAULT (unixepoch('subsecond') * 1_000_000_000);

-- Set the existing messages inserted_at_ns to sent_at_ns
UPDATE group_messages
SET inserted_at_ns = sent_at_ns;
