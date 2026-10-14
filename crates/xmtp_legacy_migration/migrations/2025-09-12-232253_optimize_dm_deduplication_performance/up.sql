-- Comprehensive optimization for DM deduplication performance
-- Creates expression index on COALESCE(dm_id, id) to support the new EXISTS-based deduplication query

-- Primary index on the COALESCE expression with last_message_ns for efficient EXISTS queries
-- Supports: WHERE COALESCE(dm_id, id) = ? AND last_message_ns > ? pattern
CREATE INDEX idx_groups_dm_coalesce_last_message ON groups(COALESCE(dm_id, id), last_message_ns DESC);
