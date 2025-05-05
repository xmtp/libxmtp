ALTER TABLE user_preferences
RENAME TO user_preferences_old;

CREATE TABLE user_preferences (
    id INTEGER PRIMARY KEY NOT NULL DEFAULT 0 CHECK (id = 0),
    hmac_key BLOB,
    hmac_key_cycled_at_ns BIGINT
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

ALTER TABLE consent_records
ADD COLUMN consented_at_ns BIGINT;

CREATE TABLE processed_sync_messages (message_id BLOB PRIMARY KEY NOT NULL);
