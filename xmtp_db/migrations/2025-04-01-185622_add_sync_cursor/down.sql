DROP TABLE processed_device_sync_messages;

ALTER TABLE consent_records DROP COLUMN consented_at_ns;

ALTER TABLE user_preferences
RENAME TO user_preferences_old;

CREATE TABLE user_preferences (
    id INTEGER PRIMARY KEY ASC NOT NULL,
    hmac_key BLOB
);

INSERT INTO
    user_preferences (id, hmac_key)
SELECT
    0,
    hmac_key
FROM
    user_preferences_old
ORDER BY
    id DESC
LIMIT
    1;

DROP TABLE user_preferences_old;
