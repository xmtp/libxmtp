DROP INDEX idx_group_messages_reference_id;
ALTER TABLE group_messages
DROP COLUMN reference_id;