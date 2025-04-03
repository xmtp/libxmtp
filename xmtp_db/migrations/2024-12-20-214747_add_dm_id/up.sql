DROP VIEW IF EXISTS conversation_list;
ALTER TABLE groups ADD COLUMN dm_id TEXT;
ALTER TABLE groups ADD COLUMN last_message_ns BIGINT;

-- Fill the dm_id column
UPDATE groups
SET dm_id = 'dm:' ||
    LOWER(
        CASE
            WHEN LOWER((SELECT inbox_id FROM identity)) < LOWER(dm_inbox_id)
            THEN (SELECT inbox_id FROM identity) || ':' || dm_inbox_id
            ELSE dm_inbox_id || ':' || (SELECT inbox_id FROM identity)
        END
    )
WHERE dm_inbox_id IS NOT NULL;

DROP INDEX IF EXISTS idx_dm_target;
ALTER TABLE groups DROP COLUMN dm_inbox_id;

-- Create a trigger to auto-update group table on insert
CREATE TRIGGER msg_inserted
AFTER INSERT ON group_messages
BEGIN
  UPDATE groups
  SET last_message_ns = (strftime('%s', 'now') * 1000000000) + (strftime('%f', 'now') * 1000000)
  WHERE id = NEW.group_id;
END;


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
        ROW_NUMBER() OVER (PARTITION BY gm.group_id ORDER BY gm.sent_at_ns DESC) AS row_num
    FROM
        group_messages gm
    WHERE
        gm.kind = 1
)
SELECT
    g.id AS id,
    g.created_at_ns,
    g.membership_state,
    g.installations_last_checked,
    g.added_by_inbox_id,
    g.welcome_id,
    g.dm_id,
    g.rotated_at_ns,
    g.conversation_type,
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
    rm.authority_id
FROM
    groups g
    LEFT JOIN ranked_messages rm
    ON g.id = rm.group_id AND rm.row_num = 1
ORDER BY COALESCE(rm.sent_at_ns, g.created_at_ns) DESC;
