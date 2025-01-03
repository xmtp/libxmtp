ALTER TABLE group_messages
ADD COLUMN parent_id BINARY;
CREATE INDEX idx_group_messages_parent_id ON group_messages(parent_id);