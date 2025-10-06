DROP VIEW IF EXISTS conversation_list;
-- DROP TRIGGER IF EXISTS msg_inserted;

UPDATE group_messages SET originator_id = 10 WHERE originator_id IS NULL;
UPDATE group_messages SET sequence_id = 0 WHERE sequence_id IS NULL;

ALTER TABLE group_messages ADD COLUMN sequence_id_new BIGINT NOT NULL;
ALTER TABLE group_messages ADD COLUMN originator_id_new BIGINT NOT NULL;

UPDATE group_messages SET sequence_id_new = sequence_id;
UPDATE group_messages SET originator_id_new = originator_id;

ALTER TABLE group_messages DROP COLUMN sequence_id;
ALTER TABLE group_messages DROP COLUMN originator_id;

ALTER TABLE group_messages RENAME COLUMN sequence_id_new TO sequence_id;
ALTER TABLE group_messages RENAME COLUMN originator_id_new TO originator_id;

--- rebuild views with sequence/originators
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
        AND gm.content_type IN (1, 4, 6, 7, 8, 9)
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
