ALTER TABLE group_messages
ADD COLUMN reference_id BINARY;
CREATE INDEX idx_group_messages_reference_id ON group_messages(reference_id);