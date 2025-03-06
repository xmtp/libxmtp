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

DELETE FROM consent_records
WHERE
    entity_type = 3;
