DROP TRIGGER IF EXISTS msg_inserted;

CREATE TRIGGER msg_inserted AFTER INSERT ON group_messages BEGIN
UPDATE groups
SET
    last_message_ns = (strftime ('%s', 'now') * 1000000000) + (strftime ('%f', 'now') * 1000000)
WHERE
    id = NEW.group_id;

END;
