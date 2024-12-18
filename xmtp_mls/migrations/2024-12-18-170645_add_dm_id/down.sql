ALTER TABLE groups DROP COLUMN dm_id;
ALTER TABLE groups DROP COLUMN last_message_ns;

DROP TRIGGER IF EXISTS msg_inserted;
