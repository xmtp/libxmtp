DROP INDEX IF EXISTS idx_message_edits_latest;
CREATE INDEX idx_message_edits_edited_message_id
    ON message_edits (edited_message_id);
