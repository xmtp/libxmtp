-- Who deleted this message under PROC-037. Set once; its content is then empty.
ALTER TABLE group_messages ADD COLUMN deleted_by_inbox_id TEXT;

-- Deletion records outlive their deletion message: rebuild without the cascade.
CREATE TABLE message_deletions_v2 (
  -- The deletion message id
  id BLOB PRIMARY KEY NOT NULL,
  group_id BLOB NOT NULL,
  -- The target id
  deleted_message_id BLOB NOT NULL,
  deleted_by_inbox_id TEXT NOT NULL,
  -- At that point in the topic
  is_super_admin_deletion BOOLEAN NOT NULL,
  -- The deletion's envelope time
  deleted_at_ns BIGINT NOT NULL,
  -- From the topic or own echo, not an archive
  authenticated BOOLEAN NOT NULL
);
-- Only a published deletion has a record (delivery_status 2 = Published).
INSERT INTO message_deletions_v2
  SELECT d.id, d.group_id, d.deleted_message_id, d.deleted_by_inbox_id,
         d.is_super_admin_deletion, d.deleted_at_ns, 1
  FROM message_deletions d JOIN group_messages m ON m.id = d.id
  WHERE m.delivery_status = 2;
DROP TABLE message_deletions;
ALTER TABLE message_deletions_v2 RENAME TO message_deletions;
CREATE INDEX message_deletions_target ON message_deletions(group_id, deleted_message_id);

-- A finished send keeps no plaintext (kind 1 = SendMessage;
-- states 4 Error, 5 Processed, 6 Superseded, 7 FollowUpFailed).
UPDATE group_intents SET data = x'' WHERE kind = 1 AND state IN (4, 5, 6, 7);
CREATE TRIGGER send_intent_plaintext_wiped
AFTER UPDATE OF state ON group_intents
WHEN NEW.kind = 1 AND NEW.state IN (4, 5, 6, 7) AND length(NEW.data) > 0
BEGIN
  UPDATE group_intents SET data = x'' WHERE id = NEW.id;
END;
