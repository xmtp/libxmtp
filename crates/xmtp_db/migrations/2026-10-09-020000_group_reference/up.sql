CREATE INDEX group_messages_group_reference
    ON group_messages(group_id, reference_id)
    WHERE reference_id IS NOT NULL;
