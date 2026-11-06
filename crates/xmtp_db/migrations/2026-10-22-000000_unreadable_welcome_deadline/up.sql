ALTER TABLE incoming_envelopes ADD COLUMN unreadable_expires_at_ns BIGINT;

-- Existing unsupported-wrapper holds already started their one-day clock.
-- Other retry deadlines may belong to pointer fetching and cannot cap it.
UPDATE incoming_envelopes
SET unreadable_expires_at_ns = retry_expires_at_ns, retry_expires_at_ns = NULL
WHERE entity_kind = 1 AND error_code = 'unsupported_welcome';
