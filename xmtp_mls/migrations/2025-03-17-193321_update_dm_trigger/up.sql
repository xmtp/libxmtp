DROP TRIGGER IF EXISTS msg_inserted;

CREATE TRIGGER msg_inserted AFTER INSERT ON group_messages BEGIN
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
