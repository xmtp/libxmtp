ALTER TABLE groups ADD COLUMN dm_id TEXT;
ALTER TABLE groups ADD COLUMN last_message_ns BIGINT;

-- Create a trigger to auto-update group table on insert;
CREATE TRIGGER msg_iserted
AFTER INSERT ON group_messages
BEGIN
  UPDATE groups
  SET last_message_ns = (strftime('%s', 'now') * 1000000000) + (strftime('%f', 'now') * 1000000)
  WHERE id = NEW.group_id;
END;
