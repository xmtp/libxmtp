ALTER TABLE user_preferences ADD COLUMN push_unregister_owed BOOLEAN NOT NULL DEFAULT FALSE;
UPDATE user_preferences SET push_unregister_owed = TRUE
WHERE push_recipient_id IS NOT NULL AND push_last_state IS NULL;
