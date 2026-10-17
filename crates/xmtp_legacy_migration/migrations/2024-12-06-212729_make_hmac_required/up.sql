ALTER TABLE user_preferences DROP COLUMN hmac_key;
ALTER TABLE user_preferences ADD COLUMN hmac_key BLOB NOT NULL;
