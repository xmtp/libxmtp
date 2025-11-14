-- For get_latest_message_times_by_sender
CREATE INDEX idx_group_messages_sender_inbox_id_sent_at
ON group_messages(sender_inbox_id, sent_at_ns);

-- For delete_expired_messages
CREATE INDEX idx_group_messages_expiration
ON group_messages(delivery_status, kind, expire_at_ns)
WHERE expire_at_ns IS NOT NULL;

-- For get_group_message_by_cursor
CREATE INDEX idx_group_messages_cursor_lookup
ON group_messages(group_id, sequence_id, originator_id);

-- For messages_newer_than
CREATE INDEX idx_group_messages_originator_sequence
ON group_messages(originator_id, sequence_id);

-- For content_type filtering
CREATE INDEX idx_group_messages_content_type
ON group_messages(content_type);

-- For delivery_status filtering
CREATE INDEX idx_group_messages_delivery_status
ON group_messages(delivery_status);
