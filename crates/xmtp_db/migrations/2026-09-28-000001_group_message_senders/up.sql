-- Keep one row per physical group and sender observed in this database.
-- Rows survive message and group deletion, so group ID reuse keeps the evidence.
CREATE TABLE group_message_senders (
    group_id BLOB NOT NULL,
    sender_inbox_id TEXT NOT NULL,
    PRIMARY KEY (group_id, sender_inbox_id)
) WITHOUT ROWID;

INSERT INTO group_message_senders (group_id, sender_inbox_id)
SELECT DISTINCT group_id, sender_inbox_id FROM group_messages;

CREATE TRIGGER group_message_senders_insert AFTER INSERT ON group_messages BEGIN
    INSERT OR IGNORE INTO group_message_senders (group_id, sender_inbox_id)
    VALUES (NEW.group_id, NEW.sender_inbox_id);
END;

CREATE TRIGGER group_message_senders_update AFTER UPDATE OF group_id, sender_inbox_id ON group_messages BEGIN
    INSERT OR IGNORE INTO group_message_senders (group_id, sender_inbox_id)
    VALUES (NEW.group_id, NEW.sender_inbox_id);
END;
