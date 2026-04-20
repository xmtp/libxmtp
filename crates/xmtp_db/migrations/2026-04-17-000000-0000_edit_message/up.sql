-- Tracks message edits for sender-authored edits.
--
-- Cleanup strategy: Edit records are automatically removed when the EditMessage
-- itself is purged from group_messages (via FK CASCADE on `id`). The target
-- message (edited_message_id) is NOT constrained or cascade-deleted because:
-- 1. The edit record serves as audit trail even if the target is already gone
-- 2. Clients may receive EditMessage before the target message (out-of-order
--    delivery) — constraining edited_message_id would reject the legitimate
--    store, breaking out-of-order sync
-- 3. Enrichment tolerates a missing target (the edit is simply skipped)
--
-- For long-term storage optimization, consider periodic cleanup of edit records
-- where the target message no longer exists in group_messages.
CREATE TABLE message_edits (
  -- Primary key: the ID of the EditMessage in the group_messages table
  id BLOB PRIMARY KEY NOT NULL,
  -- Group this edit belongs to
  group_id BLOB NOT NULL,
  -- The ID of the original message being edited
  edited_message_id BLOB NOT NULL,
  -- The inbox_id of who sent the edit
  edited_by_inbox_id TEXT NOT NULL,
  -- The replacement encoded content bytes
  edited_content_bytes BLOB NOT NULL,
  -- Timestamp when the edit was processed
  edited_at_ns BIGINT NOT NULL,
  FOREIGN KEY (id) REFERENCES group_messages(id) ON DELETE CASCADE
);

CREATE INDEX idx_message_edits_edited_message_id ON message_edits(edited_message_id);
CREATE INDEX idx_message_edits_group_id ON message_edits(group_id);
