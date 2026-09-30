-- The conversation list seeks the latest live message of each listed group:
-- `WHERE group_id = ? ... ORDER BY sent_at_ns DESC, id DESC LIMIT 1`.
-- `id` orders equal send times. `kind`, `content_type`, and `expire_at_ns`
-- let the seek skip ineligible or expired rows without a table read.
-- The new index starts with the columns of the index it replaces, so queries
-- that used `group_messages_sent_at_sort` can use it too.
DROP INDEX group_messages_sent_at_sort;
CREATE INDEX group_messages_sent_at_id_sort
    ON group_messages(group_id, sent_at_ns, id, kind, content_type, expire_at_ns);
