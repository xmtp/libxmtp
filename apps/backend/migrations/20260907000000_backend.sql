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

CREATE TABLE push_recipient (
    recipient_id bytea PRIMARY KEY CHECK (octet_length(recipient_id) = 32),
    secret_hash bytea NOT NULL CHECK (octet_length(secret_hash) = 32),
    channel smallint NOT NULL CHECK (channel BETWEEN 1 AND 3),
    delivery text NOT NULL CHECK (length(delivery) BETWEEN 1 AND 2048),
    signing_key bytea CHECK (octet_length(signing_key) BETWEEN 16 AND 64),
    metadata bytea NOT NULL CHECK (octet_length(metadata) <= 4096),
    topic_count integer NOT NULL CHECK (topic_count >= 0),
    renewed_ns bigint NOT NULL CHECK (renewed_ns >= 0)
);
CREATE INDEX push_recipient_renewed ON push_recipient (renewed_ns);

CREATE TABLE push_subscription (
    recipient_id bytea NOT NULL REFERENCES push_recipient (recipient_id) ON DELETE CASCADE,
    topic bytea NOT NULL CHECK (octet_length(topic) BETWEEN 1 AND 128),
    since_sequence_id bigint NOT NULL CHECK (since_sequence_id >= 0),
    hmac_epoch_base bigint,
    hmac_key_0 bytea CHECK (octet_length(hmac_key_0) = 42),
    hmac_key_1 bytea CHECK (octet_length(hmac_key_1) = 42),
    hmac_key_2 bytea CHECK (octet_length(hmac_key_2) = 42),
    include_commits boolean NOT NULL,
    PRIMARY KEY (recipient_id, topic)
);
CREATE INDEX push_subscription_topic ON push_subscription (topic);

CREATE TABLE push_cursor (
    singleton boolean PRIMARY KEY DEFAULT true CHECK (singleton),
    sequence_id bigint NOT NULL CHECK (sequence_id >= 0)
);
INSERT INTO push_cursor (singleton, sequence_id) VALUES (true, 0);

ALTER TABLE envelopes
    ADD COLUMN push_eligible boolean NOT NULL,
    ADD COLUMN sender_hmac bytea CHECK (octet_length(sender_hmac) = 32);
CREATE INDEX envelopes_push_eligible ON envelopes (sequence_id) WHERE push_eligible;
