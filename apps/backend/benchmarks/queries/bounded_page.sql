WITH wanted AS MATERIALIZED (
    SELECT requested.topic, requested.cursor, watermark.last_sequence_id
    FROM unnest($1::bytea[], $2::bigint[]) requested(topic, cursor)
    JOIN topic_watermark watermark ON watermark.topic = requested.topic
        AND watermark.last_sequence_id > requested.cursor
), scan_window AS MATERIALIZED (
    SELECT sequence_id, topic FROM envelopes
    WHERE sequence_id > (SELECT min(cursor) FROM wanted)
        AND sequence_id <= (SELECT max(last_sequence_id) FROM wanted)
    ORDER BY sequence_id LIMIT $4::bigint
), fast AS MATERIALIZED (
    SELECT envelope.sequence_id FROM scan_window envelope JOIN wanted
        ON wanted.topic = envelope.topic AND envelope.sequence_id > wanted.cursor
    ORDER BY envelope.sequence_id LIMIT ($3::bigint + 1)
), enough AS MATERIALIZED (
    SELECT count(*) > $3::bigint AS ready FROM fast
), candidates AS MATERIALIZED (
    SELECT sequence_id FROM fast WHERE (SELECT ready FROM enough)
    UNION ALL
    SELECT sequence_id FROM (
        SELECT envelope.sequence_id FROM wanted CROSS JOIN LATERAL (
            SELECT sequence_id FROM envelopes
            WHERE topic = wanted.topic AND sequence_id > wanted.cursor
                AND sequence_id <= wanted.last_sequence_id
            ORDER BY sequence_id LIMIT ($3::bigint + 1)
        ) envelope WHERE NOT (SELECT ready FROM enough)
        ORDER BY envelope.sequence_id LIMIT ($3::bigint + 1)
    ) fallback
), page AS (
    SELECT sequence_id FROM candidates ORDER BY sequence_id LIMIT $3::bigint
)
SELECT envelope.sequence_id, envelope.topic, envelope.server_ns, envelope.expiry_ns,
    envelope.message_hash, envelope.is_commit_or_proposal, envelope.payload,
    (SELECT count(*) > $3::bigint FROM candidates) AS has_more
FROM page JOIN envelopes envelope USING (sequence_id) ORDER BY envelope.sequence_id
