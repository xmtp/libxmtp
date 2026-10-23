DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM public.envelopes) THEN
        RAISE EXCEPTION 'partition experiment requires an empty disposable fixture';
    END IF;
    PERFORM pg_advisory_xact_lock(0, 4);
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'xmtp_bench_writer') THEN
        BEGIN
            CREATE ROLE xmtp_bench_writer LOGIN NOINHERIT;
        EXCEPTION WHEN duplicate_object OR unique_violation THEN
            IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'xmtp_bench_writer') THEN
                RAISE;
            END IF;
        END;
    END IF;
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'xmtp_bench_writer'
        AND (rolsuper OR rolcreatedb OR rolcreaterole OR rolreplication OR rolbypassrls
            OR rolinherit OR NOT rolcanlogin OR rolconfig IS NOT NULL OR rolconnlimit <> -1))
        OR EXISTS (SELECT 1 FROM pg_auth_members
            WHERE member = 'xmtp_bench_writer'::regrole) THEN
        RAISE EXCEPTION 'benchmark writer must be an unprivileged role without memberships';
    END IF;
END $$;

ALTER SEQUENCE public.envelope_sequence OWNED BY NONE;
DROP TABLE public.envelopes;
CREATE TABLE public.envelopes (
    sequence_id bigint PRIMARY KEY CHECK (sequence_id > 0),
    topic bytea NOT NULL CHECK (octet_length(topic) BETWEEN 1 AND 128),
    server_ns bigint NOT NULL CHECK (server_ns >= 0),
    expiry_ns bigint CHECK (expiry_ns >= server_ns),
    message_hash bytea NOT NULL CHECK (octet_length(message_hash) = 32),
    is_commit_or_proposal boolean NOT NULL,
    payload bytea NOT NULL,
    push_eligible boolean NOT NULL,
    sender_hmac bytea CHECK (octet_length(sender_hmac) = 32)
) PARTITION BY RANGE (sequence_id);
ALTER SEQUENCE public.envelope_sequence OWNED BY public.envelopes.sequence_id;
CREATE INDEX envelopes_topic_sequence ON public.envelopes (topic, sequence_id);
CREATE INDEX envelopes_push_eligible ON public.envelopes (sequence_id) WHERE push_eligible;

CREATE TABLE public.envelope_identity (
    topic bytea NOT NULL CHECK (octet_length(topic) BETWEEN 1 AND 128),
    message_hash bytea NOT NULL CHECK (octet_length(message_hash) = 32),
    sequence_id bigint NOT NULL UNIQUE CHECK (sequence_id > 0),
    PRIMARY KEY (topic, message_hash),
    FOREIGN KEY (sequence_id) REFERENCES public.envelopes (sequence_id)
        ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED
);

CREATE FUNCTION public.register_envelope_identity() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog AS $$
BEGIN
    INSERT INTO public.envelope_identity (topic, message_hash, sequence_id)
        VALUES (NEW.topic, NEW.message_hash, NEW.sequence_id);
    RETURN NEW;
END $$;
REVOKE ALL ON FUNCTION public.register_envelope_identity() FROM PUBLIC;
CREATE TRIGGER register_envelope_identity BEFORE INSERT ON public.envelopes
    FOR EACH ROW EXECUTE FUNCTION public.register_envelope_identity();

CREATE FUNCTION public.reject_envelope_key_update() RETURNS trigger
LANGUAGE plpgsql SET search_path = pg_catalog AS $$
BEGIN
    IF (NEW.topic, NEW.message_hash, NEW.sequence_id)
        IS DISTINCT FROM (OLD.topic, OLD.message_hash, OLD.sequence_id) THEN
        RAISE EXCEPTION 'envelope identity is immutable' USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
END $$;
REVOKE ALL ON FUNCTION public.reject_envelope_key_update() FROM PUBLIC;
CREATE TRIGGER immutable_envelope_key BEFORE UPDATE ON public.envelopes
    FOR EACH ROW EXECUTE FUNCTION public.reject_envelope_key_update();
CREATE TRIGGER immutable_identity_key BEFORE UPDATE ON public.envelope_identity
    FOR EACH ROW EXECUTE FUNCTION public.reject_envelope_key_update();

REVOKE CREATE ON SCHEMA public FROM PUBLIC;
GRANT USAGE ON SCHEMA public TO xmtp_bench_writer;
GRANT SELECT ON ALL TABLES IN SCHEMA public TO xmtp_bench_writer;
GRANT INSERT, UPDATE, DELETE ON public.topic_watermark, public.identifier_association,
    public.allocation_boundary, public.push_recipient, public.push_subscription,
    public.push_cursor TO xmtp_bench_writer;
GRANT INSERT, DELETE ON public.envelopes TO xmtp_bench_writer;
GRANT UPDATE (server_ns, expiry_ns, is_commit_or_proposal, payload, push_eligible, sender_hmac)
    ON public.envelopes TO xmtp_bench_writer;
GRANT USAGE, SELECT ON SEQUENCE public.envelope_sequence TO xmtp_bench_writer;
