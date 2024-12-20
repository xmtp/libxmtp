ALTER TABLE groups ADD COLUMN dm_id TEXT;
ALTER TABLE groups ADD COLUMN last_message_ns BIGINT;

-- Fill the dm_id column
UPDATE groups
SET dm_id = 'dm:' ||
    LOWER(
        CASE
            WHEN LOWER((SELECT inbox_id FROM identity)) < LOWER(dm_inbox_id)
            THEN (SELECT inbox_id FROM identity) || ':' || dm_inbox_id
            ELSE dm_inbox_id || ':' || (SELECT inbox_id FROM identity)
        END
    )
WHERE dm_inbox_id IS NOT NULL;

DROP INDEX IF EXISTS idx_dm_target;
ALTER TABLE groups DROP COLUMN dm_inbox_id;

-- Create a trigger to auto-update group table on insert
CREATE TRIGGER msg_inserted
AFTER INSERT ON group_messages
BEGIN
  UPDATE groups
  SET last_message_ns = (strftime('%s', 'now') * 1000000000) + (strftime('%f', 'now') * 1000000)
  WHERE id = NEW.group_id;
END;
