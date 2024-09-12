ALTER TABLE "groups"
    -- Enum of CONSENT_STATE (ALLOWED, DENIED, etc..)
    ADD COLUMN consent_state int NOT NULL DEFAULT 0;