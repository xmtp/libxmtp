CREATE TABLE conversation_invites (
    installation_id TEXT NOT NULL,
    conversation_id TEXT NOT NULL,
    created_at_ns BIGINT NOT NULL,
    direction SMALLINT NOT NULL,
    PRIMARY KEY (installation_id, conversation_id)
);