-- Tracks message deletions for soft-delete functionality.
--
-- Cleanup strategy: Deletion records are automatically removed when the DeleteMessage
-- itself is purged from group_messages (via FK CASCADE). The target message (deleted_message_id)
-- is NOT cascade-deleted because:
-- 1. The deletion record serves as audit trail even if target is already gone
-- 2. Clients may receive DeleteMessage before the target message (out-of-order delivery)
-- 3. Allows UI to show "message deleted" placeholder until full cleanup
--
-- For long-term storage optimization, consider periodic cleanup of deletion records
-- where the target message no longer exists in group_messages.
CREATE TABLE message_deletions (
  -- Primary key: the ID of the DeleteMessage in the group_messages table
  id BLOB PRIMARY KEY NOT NULL,

  -- Group this deletion belongs to
  group_id BLOB NOT NULL,

  -- The ID of the original message being deleted
  deleted_message_id BLOB NOT NULL,

  -- The inbox_id of who sent the delete message
  deleted_by_inbox_id TEXT NOT NULL,

  -- Whether the deleter was a super admin at deletion time
  is_super_admin_deletion BOOLEAN NOT NULL,

  -- Timestamp when the deletion was processed
  deleted_at_ns BIGINT NOT NULL,

  -- Foreign key to the DeleteMessage in group_messages
  FOREIGN KEY (id) REFERENCES group_messages(id) ON DELETE CASCADE
);

CREATE INDEX idx_message_deletions_deleted_message_id ON message_deletions(deleted_message_id);
CREATE INDEX idx_message_deletions_group_id ON message_deletions(group_id);
