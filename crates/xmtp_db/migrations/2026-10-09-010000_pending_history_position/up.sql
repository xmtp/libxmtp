CREATE INDEX group_messages_pending_history_position
    ON group_messages(group_id, sent_at_ns, id)
    WHERE delivery_status IN (1, 3);
