CREATE TABLE "consent_records"(
    -- Enum of the CONSENT_TYPE (GROUP_ID, INBOX_ID, etc..)
    "entity_type" int NOT NULL,
    -- Enum of CONSENT_STATE (ALLOWED, DENIED, etc..)
    "state" int NOT NULL,
    -- The entity of what has consent (0x00 etc..)
    "entity" text NOT NULL,
    PRIMARY KEY (entity_type, entity)
);