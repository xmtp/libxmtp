-- Tracks message edits for edit functionality.
-- Note: We intentionally don't add a FK on original_message_id because edits can arrive
-- before the original message (out-of-order delivery). Orphaned edits are cleaned up
-- at application level during enrichment when the original message doesn't exist.
CREATE TABLE message_edits (
  -- Primary key: the ID of the EditMessage in the group_messages table
  id BLOB PRIMARY KEY NOT NULL,

  -- Group this edit belongs to
  group_id BLOB NOT NULL,

  -- The ID of the original message being edited
  original_message_id BLOB NOT NULL,

  -- The inbox_id of who sent the edit message
  edited_by_inbox_id TEXT NOT NULL,

  -- The edited content (serialized EncodedContent)
  edited_content BLOB NOT NULL,

  -- Timestamp when the edit was processed
  edited_at_ns BIGINT NOT NULL,

  -- Foreign key to the EditMessage in group_messages (cleanup when edit message deleted)
  FOREIGN KEY (id) REFERENCES group_messages(id) ON DELETE CASCADE
);

-- Composite index for efficient "get latest edit per message" queries
CREATE INDEX idx_message_edits_original_timestamp ON message_edits(original_message_id, edited_at_ns DESC);
CREATE INDEX idx_message_edits_group_id ON message_edits(group_id);
