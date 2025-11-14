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


-- Groups

-- 1. CRITICAL: Consent record lookups (used in almost every find_groups call with JOINs)
CREATE INDEX IF NOT EXISTS idx_consent_records_entity ON consent_records(entity);

-- 2. CRITICAL: DM deduplication (optimizes expensive EXISTS subquery in find_groups)
CREATE INDEX IF NOT EXISTS idx_groups_dm_dedup ON groups(
    COALESCE(dm_id, id),
    COALESCE(last_message_ns, 0) DESC,
    id
);

-- 3. HIGH: Conversation type filtering (used in most group queries)
CREATE INDEX IF NOT EXISTS idx_groups_conversation_type ON groups(conversation_type);

-- 4. HIGH: Find group by welcome/sequence ID (direct lookup pattern)
CREATE INDEX IF NOT EXISTS idx_groups_sequence_originator ON groups(sequence_id, originator_id)
    WHERE sequence_id IS NOT NULL;

-- 5. MEDIUM-HIGH: Remote commit log queries (used in fork detection and readd flows)
CREATE INDEX IF NOT EXISTS idx_remote_commit_log_group_commit ON remote_commit_log(group_id, commit_sequence_id DESC);

CREATE INDEX idx_groups_dm_id ON groups(dm_id) WHERE dm_id IS NOT NULL;

-- Consent Records

-- CREATE INDEX idx_consent_records_entity ON consent_records(entity, entity_type);

ALTER TABLE groups ADD COLUMN id_hex TEXT GENERATED ALWAYS AS (lower(hex(id))) STORED NOT NULL;
CREATE INDEX idx_groups_id_hex ON groups(id_hex);
