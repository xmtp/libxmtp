DROP INDEX public.envelopes_topic_sequence;
CREATE INDEX envelopes_topic_sequence ON public.envelopes (topic, sequence_id)
    INCLUDE (server_ns, expiry_ns, message_hash, is_commit_or_proposal);
