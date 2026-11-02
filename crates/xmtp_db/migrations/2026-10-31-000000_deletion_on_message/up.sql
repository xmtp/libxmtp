-- Who deleted this message. Set once; its content is then empty.
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

-- Mark and empty every target that a kept record deletes, as the old
-- read-time check did, so nothing it hid becomes readable. That check read
-- the wire type: authority xmtp.org, major version 1, and a deletable type
-- id. An encoded content starts with that type, with or without a minor
-- version, so the prefixes below are those types.
WITH deletable_type_prefix(prefix) AS (VALUES
  (x'0a120a08786d74702e6f72671204746578741801'),
  (x'0a140a08786d74702e6f7267120474657874180120'),
  (x'0a160a08786d74702e6f726712086d61726b646f776e1801'),
  (x'0a180a08786d74702e6f726712086d61726b646f776e180120'),
  (x'0a130a08786d74702e6f726712057265706c791801'),
  (x'0a150a08786d74702e6f726712057265706c79180120'),
  (x'0a180a08786d74702e6f7267120a6174746163686d656e741801'),
  (x'0a1a0a08786d74702e6f7267120a6174746163686d656e74180120'),
  (x'0a240a08786d74702e6f7267121672656d6f74655374617469634174746163686d656e741801'),
  (x'0a260a08786d74702e6f7267121672656d6f74655374617469634174746163686d656e74180120'),
  (x'0a220a08786d74702e6f726712147472616e73616374696f6e5265666572656e63651801'),
  (x'0a240a08786d74702e6f726712147472616e73616374696f6e5265666572656e6365180120'),
  (x'0a1d0a08786d74702e6f7267120f77616c6c657453656e6443616c6c731801'),
  (x'0a1f0a08786d74702e6f7267120f77616c6c657453656e6443616c6c73180120'),
  (x'0a290a08786d74702e6f7267121b6d756c746952656d6f74655374617469634174746163686d656e741801'),
  (x'0a2b0a08786d74702e6f7267121b6d756c746952656d6f74655374617469634174746163686d656e74180120'))
UPDATE group_messages
SET deleted_by_inbox_id = (
      SELECT d.deleted_by_inbox_id FROM message_deletions d
      WHERE d.deleted_message_id = group_messages.id
        AND d.group_id = group_messages.group_id
        AND (d.deleted_by_inbox_id = group_messages.sender_inbox_id
             OR d.is_super_admin_deletion)
      ORDER BY d.deleted_at_ns, d.id LIMIT 1),
    decrypted_message_bytes = x''
WHERE kind = 1
  AND deleted_by_inbox_id IS NULL
  AND EXISTS (
      SELECT 1 FROM message_deletions d
      WHERE d.deleted_message_id = group_messages.id
        AND d.group_id = group_messages.group_id
        AND (d.deleted_by_inbox_id = group_messages.sender_inbox_id
             OR d.is_super_admin_deletion))
  AND EXISTS (
      SELECT 1 FROM deletable_type_prefix t
      WHERE substr(group_messages.decrypted_message_bytes, 1, length(t.prefix)) = t.prefix);

-- A finished send keeps no plaintext (kind 1 = SendMessage;
-- states 4 Error, 5 Processed, 6 Superseded, 7 FollowUpFailed).
UPDATE group_intents SET data = x'' WHERE kind = 1 AND state IN (4, 5, 6, 7);
CREATE TRIGGER send_intent_plaintext_wiped
AFTER UPDATE OF state ON group_intents
WHEN NEW.kind = 1 AND NEW.state IN (4, 5, 6, 7) AND length(NEW.data) > 0
BEGIN
  UPDATE group_intents SET data = x'' WHERE id = NEW.id;
END;
