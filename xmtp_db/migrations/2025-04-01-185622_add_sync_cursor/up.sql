ALTER TABLE user_preferences
RENAME TO user_preferences_old;

CREATE TABLE user_preferences (
    id INTEGER PRIMARY KEY NOT NULL DEFAULT 0 CHECK (id = 0),
    hmac_key BLOB,
    sync_cursor_group_id BLOB,
    sync_cursor_offset INT NOT NULL DEFAULT 0
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
