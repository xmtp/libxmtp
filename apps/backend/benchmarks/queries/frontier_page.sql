WITH wanted AS (
    SELECT * FROM unnest($1::bytea[], $2::bigint[]) AS requested(topic, cursor)
), firsts AS MATERIALIZED (
    SELECT wanted.topic, envelope.sequence_id FROM wanted CROSS JOIN LATERAL (
        SELECT sequence_id FROM envelopes
        WHERE topic = wanted.topic AND sequence_id > wanted.cursor
        ORDER BY sequence_id LIMIT 1
    ) envelope
), bound AS MATERIALIZED (
    SELECT coalesce((SELECT sequence_id FROM firsts ORDER BY sequence_id
        LIMIT 1 OFFSET $3::bigint), 9223372036854775807::bigint) AS sequence_id
), candidates AS MATERIALIZED (
    SELECT envelope.sequence_id FROM firsts CROSS JOIN bound CROSS JOIN LATERAL (
        SELECT sequence_id FROM envelopes
        WHERE topic = firsts.topic AND sequence_id >= firsts.sequence_id
            AND sequence_id <= bound.sequence_id
        ORDER BY sequence_id LIMIT ($3::bigint + 1)
    ) envelope WHERE firsts.sequence_id <= bound.sequence_id
    ORDER BY envelope.sequence_id LIMIT ($3::bigint + 1)
), page AS (
    SELECT sequence_id FROM candidates ORDER BY sequence_id LIMIT $3::bigint
)
SELECT envelope.sequence_id, envelope.topic, envelope.server_ns, envelope.expiry_ns,
    envelope.message_hash, envelope.is_commit_or_proposal, envelope.payload,
    (SELECT count(*) > $3::bigint FROM candidates) AS has_more
FROM page JOIN envelopes envelope USING (sequence_id) ORDER BY envelope.sequence_id
