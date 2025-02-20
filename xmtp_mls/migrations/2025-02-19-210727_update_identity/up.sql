-- Remove the existing pkey on wallet_addresses
ALTER TABLE wallet_addresses DROP CONSTRAINT wallet_addresses_pkey;

-- Change the name to identity_cache
ALTER TABLE wallet_addresses
RENAME TO identity_cache;

-- rename wallet_address to identity
-- add identity_kind like in consent_records below
ALTER TABLE identity_cache
RENAME COLUMN wallet_address TO identity,
ADD COLUMN identity_kind INT NOT NULL DEFAULT 1;

-- Add new composite primary key
ALTER TABLE identity_cache
ADD PRIMARY KEY (identity, identity_kind);

-- Add a new identity kind (Ethereum, Passkey, Solana, Sui...)
ALTER TABLE consent_records
ADD COLUMN identity_kind INT;

-- Set all the current Identities to Ethereum, since that's all we supported before now
UPDATE consent_records
SET identity_kind = 1
WHERE consent_type = 3;

-- Add a constraint so that the kind must be present if the type is "Identity"
ALTER TABLE consent_records
ADD CONSTRAINT chk_identity_kind_not_null
CHECK ((consent_type = 3 AND identity_kind IS NOT NULL) OR (consent_type != 3 AND identity_kind IS NULL));
