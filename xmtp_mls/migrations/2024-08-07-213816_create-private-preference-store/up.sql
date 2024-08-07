CREATE TABLE "private_preferences"(
    -- The inbox_id the private preferences are owned by
    "inbox_id" text NOT NULL,
    -- Enum of the PRIVATE_PREFERENCE_TYPE (GROUP, INBOX, etc..)
    "valueType" int NOT NULL,
    -- Enum of PREFERENCE_STATE (ALLOWED, DENIED, etc..)
    "state" int NOT NULL,
    -- The value of what has a preference
    "value" text NOT NULL,
    PRIMARY KEY (valueType, value)
);