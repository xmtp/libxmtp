-- Held-usage reads sum this stored length from the covering index below, so
-- they never load envelope bytes. Read it only through that index: in the table
-- row it follows the envelope, so a table read walks the envelope's overflow pages.
ALTER TABLE incoming_envelopes ADD COLUMN envelope_bytes BIGINT NOT NULL DEFAULT 0;
UPDATE incoming_envelopes SET envelope_bytes = length(envelope);

-- The predicate columns make the index covering for held-usage reads.
CREATE INDEX incoming_held_envelopes
    ON incoming_envelopes(
        entity_kind, entity_id, sequence_id, envelope_bytes,
        blocked, retry_expires_at_ns, unreadable_expires_at_ns
    )
    WHERE blocked OR retry_expires_at_ns IS NOT NULL OR unreadable_expires_at_ns IS NOT NULL;
