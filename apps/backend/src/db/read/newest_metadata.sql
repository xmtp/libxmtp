SELECT e.sequence_id, e.topic, e.server_ns, e.expiry_ns,
    e.message_hash, e.is_commit_or_proposal
FROM topic_watermark w JOIN envelopes e
    ON e.sequence_id = w.last_sequence_id AND e.topic = w.topic
WHERE w.topic = ANY($1::bytea[])
