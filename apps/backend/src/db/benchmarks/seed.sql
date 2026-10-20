WITH inputs AS (
    SELECT id, mod(id * 48271 + $7::bigint, 2147483647) AS random
    FROM generate_series($1::bigint, $2::bigint) id
), assigned AS (
    SELECT id, CASE
        WHEN id <= $4::bigint THEN id - 1
        WHEN mod(random, 100) < 50 THEN mod(random / 100, $5::bigint)
        WHEN mod(random, 100) < 90 OR id > $3::bigint / 2
            THEN $5 + mod(random / 100, $6::bigint - $5)
        ELSE $6 + mod(random / 100, $4 - $6)
    END AS topic_id
    FROM inputs
)
INSERT INTO envelopes (sequence_id, topic, server_ns, expiry_ns, message_hash,
    is_commit_or_proposal, payload, push_eligible, sender_hmac)
SELECT id, '\x00'::bytea || substring(sha256(int8send(topic_id)) for 16),
    1700000000000000000 + id * 1000000,
    CASE WHEN mod(id, 20) <> 0 THEN 1707776000000000000 + id * 1000000 END,
    sha256(overlay(payloads.payload placing int8send(id) from 1 for 8)), mod(id, 20) = 0,
    overlay(payloads.payload placing int8send(id) from 1 for 8),
    mod(id, 10) <> 0, sha256(int8send(id))
FROM assigned JOIN benchmark_payload payloads ON payloads.payload_id = mod(assigned.id, 100)
ORDER BY id
