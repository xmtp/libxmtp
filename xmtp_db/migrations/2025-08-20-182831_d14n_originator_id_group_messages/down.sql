DROP VIEW IF EXISTS conversation_list;
DROP TRIGGER IF EXISTS msg_inserted;

CREATE TABLE group_messages_old (
    id BLOB PRIMARY KEY,
    group_id BLOB NOT NULL,
    decrypted_message_bytes BLOB NOT NULL,
    sent_at_ns BIGINT NOT NULL,
    kind INTEGER NOT NULL,
    sender_installation_id BLOB NOT NULL,
    sender_inbox_id TEXT NOT NULL,
    delivery_status INTEGER NOT NULL,
    content_type INTEGER NOT NULL,
    version_minor INTEGER NOT NULL,
    version_major INTEGER NOT NULL,
    authority_id TEXT NOT NULL,
    reference_id BLOB,
    sequence_id BIGINT,
    originator_id BIGINT,
    expire_at_ns BIGINT,
    FOREIGN KEY (group_id) REFERENCES "groups"(id)
);

INSERT INTO group_messages_old SELECT * FROM group_messages;
DROP TABLE group_messages;
ALTER TABLE group_messages_old RENAME TO group_messages;

UPDATE group_messages SET originator_id = null WHERE originator_id IS 10;
UPDATE group_messages SET sequence_id = null WHERE sequence_id IS 0;

-- rebuild triggers
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
        AND gm.content_type IN (1, 4, 6, 7, 8, 9)
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
    rm.authority_id
FROM
    groups g
    LEFT JOIN ranked_messages rm
    ON g.id = rm.group_id AND rm.row_num = 1
ORDER BY COALESCE(rm.sent_at_ns, g.created_at_ns) DESC;
