-- Bound the ordered eligible-envelope scan before joining subscriptions by
-- topic and sorting fan-out by its keyset. The outer lateral join preserves
-- the bounds even when no subscription matches. No payload leaves this query.
WITH window_rows AS MATERIALIZED (
    SELECT sequence_id, topic, server_ns, sender_hmac, is_commit_or_proposal
    FROM envelopes
    WHERE push_eligible AND sequence_id > $1 AND sequence_id <= $2
    ORDER BY sequence_id LIMIT $3
), bounds AS (
    SELECT min(sequence_id) AS first_id, max(sequence_id) AS last_id FROM window_rows
)
SELECT b.first_id AS "first_id?", b.last_id AS "last_id?",
    d.sequence_id AS "sequence_id?", d.topic AS "topic?", d.server_ns AS "server_ns?",
    d.sender_hmac AS "sender_hmac?", d.recipient_id AS "recipient_id?",
    d.hmac_epoch_base AS "hmac_epoch_base?", d.hmac_key_0 AS "hmac_key_0?",
    d.hmac_key_1 AS "hmac_key_1?", d.hmac_key_2 AS "hmac_key_2?",
    d.channel AS "channel?", d.delivery AS "delivery?",
    d.signing_key AS "signing_key?", d.metadata AS "metadata?"
FROM bounds b
LEFT JOIN LATERAL (
    SELECT w.sequence_id, w.topic, w.server_ns, w.sender_hmac,
        s.recipient_id, s.hmac_epoch_base, s.hmac_key_0, s.hmac_key_1, s.hmac_key_2,
        r.channel, r.delivery, r.signing_key, r.metadata
    FROM window_rows w
    JOIN push_subscription s ON s.topic = w.topic
        AND w.sequence_id > s.since_sequence_id
        AND (NOT w.is_commit_or_proposal OR s.include_commits)
    JOIN push_recipient r ON r.recipient_id = s.recipient_id
    WHERE (w.sequence_id, s.recipient_id) > ($4, $5)
    ORDER BY w.sequence_id, s.recipient_id LIMIT $6
) d ON true
ORDER BY d.sequence_id, d.recipient_id
