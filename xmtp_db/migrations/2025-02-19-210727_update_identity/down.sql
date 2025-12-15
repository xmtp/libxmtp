-- This file should undo anything in `up.sql`
-- Note: Deleted consent_records (entity_type = 3) cannot be recovered

-- Recreate wallet_addresses table from identity_cache
CREATE TABLE wallet_addresses(
    inbox_id TEXT NOT NULL,
    wallet_address TEXT PRIMARY KEY NOT NULL
);

INSERT INTO
    wallet_addresses (inbox_id, wallet_address)
SELECT
    inbox_id,
    identity
FROM
    identity_cache
WHERE
    identity_kind = 1;

CREATE INDEX idx_wallet_inbox_id ON wallet_addresses(inbox_id);

-- Drop identity_cache table
DROP TABLE identity_cache;
