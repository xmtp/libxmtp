DROP INDEX group_messages_sent_at_id_sort;
CREATE INDEX group_messages_sent_at_sort ON group_messages(group_id, sent_at_ns);
