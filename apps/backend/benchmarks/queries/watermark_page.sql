WITH wanted AS (
    SELECT requested.topic, requested.cursor, watermark.last_sequence_id
    FROM unnest($1::bytea[], $2::bigint[]) AS requested(topic, cursor)
    JOIN topic_watermark watermark ON watermark.topic = requested.topic
        AND watermark.last_sequence_id > requested.cursor
), candidates AS MATERIALIZED (
    SELECT envelope.sequence_id FROM wanted CROSS JOIN LATERAL (
        SELECT sequence_id FROM envelopes
        WHERE topic = wanted.topic AND sequence_id > wanted.cursor
            AND sequence_id <= wanted.last_sequence_id
        ORDER BY sequence_id LIMIT ($3::bigint + 1)
    ) AS envelope ORDER BY envelope.sequence_id LIMIT ($3::bigint + 1)
), page AS (
    SELECT sequence_id FROM candidates ORDER BY sequence_id LIMIT $3::bigint
)
SELECT envelope.sequence_id, envelope.topic, envelope.server_ns, envelope.expiry_ns,
    envelope.message_hash, envelope.is_commit_or_proposal, envelope.payload,
    (SELECT count(*) > $3::bigint FROM candidates) AS has_more
FROM page JOIN envelopes AS envelope USING (sequence_id) ORDER BY envelope.sequence_id
