DROP INDEX idx_group_messages_parent_id;
ALTER TABLE group_messages
DROP COLUMN parent_id;