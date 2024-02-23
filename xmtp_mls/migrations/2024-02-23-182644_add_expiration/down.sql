DROP INDEX IF EXISTS key_store_expiration;

ALTER TABLE openmls_key_store
DROP COLUMN expiration;
