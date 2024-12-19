ALTER TABLE groups DROP COLUMN dm_id;
ALTER TABLE groups DROP COLUMN last_message_ns;
ALTER TABLE groups ADD COLUMN dm_inbox_id TEXT;

DROP TRIGGER IF EXISTS msg_inserted;
