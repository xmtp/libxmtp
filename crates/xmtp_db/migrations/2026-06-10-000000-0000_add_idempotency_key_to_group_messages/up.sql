-- Add idempotency_key to group_messages.
--
-- The message id is derived from this key (see `calculate_message_id`), which
-- historically was always the send timestamp (`sent_at_ns`). Exposing it lets
-- callers make application-level retries idempotent: re-sending identical
-- content with the same key yields the same message id, deduped by the PK.
--
-- The column is NOT NULL so it is always present (single source of truth). The
-- constant '' default only exists to satisfy the ALTER for pre-existing rows and
-- is immediately overwritten by the backfill below; new inserts always supply a
-- value via the Insertable, so '' never persists.
ALTER TABLE group_messages
ADD COLUMN idempotency_key TEXT NOT NULL DEFAULT '';

-- Backfill existing rows with the timestamp, matching the historical key.
UPDATE group_messages
SET idempotency_key = CAST(sent_at_ns AS TEXT);
