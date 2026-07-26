-- Reverses 0000_init. Indexes are owned by their tables and disappear with them,
-- so only the view, the trigger and its function, and the tables are dropped here.

DROP VIEW IF EXISTS conversation_list;

DROP TRIGGER IF EXISTS msg_inserted ON group_messages;
DROP FUNCTION IF EXISTS msg_inserted_fn();

DROP TABLE IF EXISTS d14n_migration_cutover;
DROP TABLE IF EXISTS user_preferences;
DROP TABLE IF EXISTS tasks;
DROP TABLE IF EXISTS processed_device_sync_messages;
DROP TABLE IF EXISTS pending_remove;
DROP TABLE IF EXISTS readd_status;
DROP TABLE IF EXISTS remote_commit_log;
DROP TABLE IF EXISTS local_commit_log;
DROP TABLE IF EXISTS key_package_history;
DROP TABLE IF EXISTS refresh_state;
DROP TABLE IF EXISTS icebox_dependencies;
DROP TABLE IF EXISTS icebox;
DROP TABLE IF EXISTS group_intents;
DROP TABLE IF EXISTS message_deletions;
DROP TABLE IF EXISTS group_messages;
DROP TABLE IF EXISTS "groups";
DROP TABLE IF EXISTS consent_records;
DROP TABLE IF EXISTS association_state;
DROP TABLE IF EXISTS identity_updates;
DROP TABLE IF EXISTS identity_cache;
DROP TABLE IF EXISTS identity;
DROP TABLE IF EXISTS openmls_key_value;
DROP TABLE IF EXISTS openmls_key_store;
