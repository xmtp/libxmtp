-- Reverse the migration by recreating the table without inserted_at_ns

-- Step 0: Drop views that depend on group_messages table
DROP VIEW IF EXISTS conversation_list;

-- Step 1: Drop the trigger
DROP TRIGGER IF EXISTS msg_inserted;

-- Step 2: Rename current table
ALTER TABLE group_messages RENAME TO group_messages_new;

-- Step 3: Recreate original table without inserted_at_ns
CREATE TABLE group_messages (
    id BLOB PRIMARY KEY NOT NULL,
    group_id BLOB NOT NULL,
    decrypted_message_bytes BLOB NOT NULL,
    sent_at_ns BIGINT NOT NULL,
    kind INTEGER NOT NULL DEFAULT 1,
    sender_installation_id BLOB NOT NULL,
    sender_inbox_id TEXT NOT NULL,
    delivery_status INTEGER NOT NULL DEFAULT 0,
    content_type INTEGER NOT NULL DEFAULT 0,
    version_major INTEGER NOT NULL DEFAULT 0,
    version_minor INTEGER NOT NULL DEFAULT 0,
    authority_id TEXT NOT NULL,
    reference_id BLOB,
    originator_id BIGINT NOT NULL,
    sequence_id BIGINT NOT NULL,
    expire_at_ns BIGINT,
    FOREIGN KEY (group_id) REFERENCES groups(id)
);

-- Step 4: Copy data back (excluding inserted_at_ns)
INSERT INTO group_messages (
    id, group_id, decrypted_message_bytes, sent_at_ns, kind,
    sender_installation_id, sender_inbox_id, delivery_status,
    content_type, version_major, version_minor, authority_id,
    reference_id, originator_id, sequence_id, expire_at_ns
)
SELECT
    id, group_id, decrypted_message_bytes, sent_at_ns, kind,
    sender_installation_id, sender_inbox_id, delivery_status,
    content_type, version_major, version_minor, authority_id,
    reference_id, originator_id, sequence_id, expire_at_ns
FROM group_messages_new;

-- Step 5: Drop new table
DROP TABLE group_messages_new;

-- Step 6: Recreate original indexes (that existed before the up migration ran)
CREATE INDEX group_messages_sent_at_sort ON group_messages(group_id, sent_at_ns);
CREATE INDEX group_messages_sent_at_sort_desc ON group_messages(group_id, sent_at_ns DESC);
CREATE INDEX group_messages_reference_id ON group_messages(reference_id);

-- Step 7: Recreate trigger
CREATE TRIGGER msg_inserted AFTER INSERT ON group_messages FOR EACH ROW BEGIN
UPDATE groups
SET
    last_message_ns = NEW.sent_at_ns
WHERE
    id = NEW.group_id
    AND (
        last_message_ns IS NULL
        OR NEW.sent_at_ns > last_message_ns
    );
END;

-- Step 8: Recreate conversation_list view
CREATE VIEW conversation_list AS
WITH ranked_messages AS (
    SELECT
        gm.group_id,
        gm.id AS message_id,
        gm.decrypted_message_bytes,
        gm.sent_at_ns,
        gm.kind AS message_kind,
        gm.sender_installation_id,
        gm.sender_inbox_id,
        gm.delivery_status,
        gm.content_type,
        gm.version_major,
        gm.version_minor,
        gm.authority_id,
        gm.sequence_id,
        gm.originator_id,
        ROW_NUMBER() OVER (PARTITION BY gm.group_id ORDER BY gm.sent_at_ns DESC) AS row_num
    FROM
        group_messages gm
    WHERE
        gm.kind = 1
        AND gm.content_type IN (0, 1, 4, 6, 7, 8, 9, 10)
)
SELECT
    g.id AS id,
    g.created_at_ns,
    g.membership_state,
    g.installations_last_checked,
    g.added_by_inbox_id,
    g.sequence_id as welcome_sequence_id,
    g.dm_id,
    g.rotated_at_ns,
    g.conversation_type,
    g.is_commit_log_forked,
    rm.message_id,
    rm.decrypted_message_bytes,
    rm.sent_at_ns,
    rm.message_kind,
    rm.sender_installation_id,
    rm.sender_inbox_id,
    rm.delivery_status,
    rm.content_type,
    rm.version_major,
    rm.version_minor,
    rm.authority_id,
    rm.sequence_id,
    rm.originator_id
FROM
    groups g
    LEFT JOIN ranked_messages rm
    ON g.id = rm.group_id AND rm.row_num = 1
ORDER BY COALESCE(rm.sent_at_ns, g.created_at_ns) DESC;
