CREATE INDEX idx_local_commit_log_group_id ON local_commit_log(group_id);
CREATE INDEX idx_remote_commit_log_group_id ON remote_commit_log(group_id);
CREATE INDEX idx_group_messages_expire_at_ns ON group_messages(expire_at_ns) WHERE expire_at_ns IS NOT NULL;
