CREATE SEQUENCE envelope_sequence AS bigint
    START WITH 1 INCREMENT BY 1 MINVALUE 1 MAXVALUE 9223372036854775807 CACHE 1 NO CYCLE;

CREATE TABLE envelopes (
    sequence_id bigint PRIMARY KEY CHECK (sequence_id > 0),
    topic bytea NOT NULL CHECK (octet_length(topic) BETWEEN 1 AND 128),
    server_ns bigint NOT NULL CHECK (server_ns >= 0),
    expiry_ns bigint CHECK (expiry_ns >= server_ns),
    message_hash bytea NOT NULL CHECK (octet_length(message_hash) = 32),
    is_commit_or_proposal boolean NOT NULL,
    payload bytea NOT NULL,
    UNIQUE (topic, message_hash)
);
ALTER SEQUENCE envelope_sequence OWNED BY envelopes.sequence_id;
CREATE INDEX envelopes_topic_sequence ON envelopes (topic, sequence_id);

CREATE TABLE topic_watermark (
    topic bytea PRIMARY KEY CHECK (octet_length(topic) BETWEEN 1 AND 128),
    last_sequence_id bigint NOT NULL CHECK (last_sequence_id > 0)
);

CREATE TABLE identifier_association (
    identifier text NOT NULL,
    identifier_kind smallint NOT NULL,
    inbox_id bytea NOT NULL CHECK (octet_length(inbox_id) = 32),
    association_sequence_id bigint NOT NULL CHECK (association_sequence_id > 0),
    revocation_sequence_id bigint CHECK (revocation_sequence_id > association_sequence_id),
    PRIMARY KEY (identifier, identifier_kind, inbox_id)
);
CREATE INDEX identifier_association_active
    ON identifier_association (identifier, identifier_kind, association_sequence_id DESC)
    WHERE revocation_sequence_id IS NULL;

CREATE TABLE allocation_boundary (
    singleton boolean PRIMARY KEY CHECK (singleton),
    closed_sequence_id bigint NOT NULL CHECK (closed_sequence_id >= 0)
);
INSERT INTO allocation_boundary VALUES (true, 0);
