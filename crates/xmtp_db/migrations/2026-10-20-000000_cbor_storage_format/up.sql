-- Marks a database whose OpenMLS key store values and serialized columns are
-- CBOR. It changes no schema. `XmtpDb::init` rejects an existing database that
-- lacks this migration and holds data (`StorageError::OldEncodingDatabase`)
-- before it runs any migration.
SELECT 1;
