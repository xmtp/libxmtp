DROP TRIGGER send_intent_plaintext_wiped;
DROP TABLE deletion_backfill;
CREATE TABLE message_deletions_v1 (
  id BLOB PRIMARY KEY NOT NULL,
  group_id BLOB NOT NULL,
  deleted_message_id BLOB NOT NULL,
  deleted_by_inbox_id TEXT NOT NULL,
  is_super_admin_deletion BOOLEAN NOT NULL,
  deleted_at_ns BIGINT NOT NULL,
  FOREIGN KEY (id) REFERENCES group_messages(id) ON DELETE CASCADE
);
INSERT INTO message_deletions_v1
  SELECT d.id, d.group_id, d.deleted_message_id, d.deleted_by_inbox_id,
         d.is_super_admin_deletion, d.deleted_at_ns
  FROM message_deletions d WHERE d.id IN (SELECT id FROM group_messages);
DROP TABLE message_deletions;
ALTER TABLE message_deletions_v1 RENAME TO message_deletions;
CREATE INDEX idx_message_deletions_deleted_message_id ON message_deletions(deleted_message_id);
CREATE INDEX idx_message_deletions_group_id ON message_deletions(group_id);
ALTER TABLE group_messages DROP COLUMN deleted_by_inbox_id;
