CREATE INDEX group_messages_group_history_position
    ON group_messages(group_id, sent_at_ns, delivery_sequence)
    WHERE delivery_sequence IS NOT NULL;
