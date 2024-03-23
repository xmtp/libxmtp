ALTER TABLE openmls_key_store
ADD COLUMN expire_at_s BIGINT;

CREATE INDEX IF NOT EXISTS key_store_expiration ON openmls_key_store(expire_at_s);
