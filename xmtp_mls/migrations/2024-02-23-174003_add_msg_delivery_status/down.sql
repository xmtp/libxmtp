-- Copy and replace is only necesasry for SQLite as SQLite does not support DROP COLUMN directly.
BEGIN TRANSACTION;
CREATE TEMPORARY TABLE backup_group(id BLOB PRIMARY KEY NOT NULL, created_at_ns BIGINT NOT NULL, membership_state INT NOT NULL, installations_last_checked BIGINT NOT NULL);
INSERT INTO backup_group SELECT id, created_at_ns, membership_state, installations_last_checked FROM groups;
DROP TABLE groups;
CREATE TABLE groups(id BLOB PRIMARY KEY NOT NULL, created_at_ns BIGINT NOT NULL, membership_state INT NOT NULL, installations_last_checked BIGINT NOT NULL);
INSERT INTO groups SELECT id, created_at_ns, membership_state, installations_last_checked FROM backup_group;
DROP TABLE backup_group;
COMMIT;
