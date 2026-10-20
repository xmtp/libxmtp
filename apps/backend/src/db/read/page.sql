WITH wanted AS (
    SELECT * FROM unnest($1::bytea[], $2::bigint[]) AS r(topic, cursor)
), candidates AS MATERIALIZED (
    SELECT e.sequence_id FROM wanted CROSS JOIN LATERAL (
        SELECT sequence_id FROM envelopes
        WHERE topic = wanted.topic AND sequence_id > wanted.cursor
        ORDER BY sequence_id LIMIT ($3::bigint + 1)
    ) AS e ORDER BY e.sequence_id LIMIT ($3::bigint + 1)
), page AS (
    SELECT sequence_id FROM candidates ORDER BY sequence_id LIMIT $3::bigint
)
SELECT e.sequence_id, e.topic, e.server_ns, e.expiry_ns,
    e.message_hash, e.is_commit_or_proposal, e.payload,
    (SELECT count(*) > $3::bigint FROM candidates) AS "has_more!"
FROM page JOIN envelopes AS e USING (sequence_id) ORDER BY e.sequence_id
