-- Change wallet_addresses to identity_cache
CREATE TABLE identity_cache (
    inbox_id TEXT NOT NULL,
    identity TEXT NOT NULL,
    identity_kind INT NOT NULL,
    PRIMARY KEY (identity, identity_kind)
);

INSERT INTO
    identity_cache (inbox_id, identity, identity_kind)
SELECT
    inbox_id,
    wallet_address,
    1
FROM
    wallet_addresses;

-- Remove wallet_addresses
DROP TABLE wallet_addresses;

-- Add a new identity kind (Ethereum, Passkey, Solana, Sui...)
ALTER TABLE consent_records
ADD COLUMN identity_kind INT;

-- Set all the current Identities to Ethereum, since that's all we supported before now
UPDATE consent_records
SET
    identity_kind = 1
WHERE
    entity_type = 3;

CREATE TABLE consent_records_new (
    entity_type int NOT NULL,
    state int NOT NULL,
    entity text NOT NULL,
    identity_kind INT,
    PRIMARY KEY (entity_type, entity),
    CHECK (
        (
            entity_type = 3
            AND identity_kind IS NOT NULL
        )
        OR (
            entity_type != 3
            AND identity_kind IS NULL
        )
    )
);

INSERT INTO
    consent_records_new (entity_type, state, entity, identity_kind)
SELECT
    entity_type,
    state,
    entity,
    CASE
        WHEN entity_type = 3 THEN 1
        ELSE NULL
    END
FROM
    consent_records;

-- Drop the old table
DROP TABLE consent_records;

-- Rename the new table to the original name
ALTER TABLE consent_records_new
RENAME TO consent_records;
