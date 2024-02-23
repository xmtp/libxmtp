ALTER TABLE openmls_key_store
ADD COLUMN expiration BIGINT;

CREATE INDEX IF NOT EXISTS key_store_expiration ON openmls_key_store(expiration);
