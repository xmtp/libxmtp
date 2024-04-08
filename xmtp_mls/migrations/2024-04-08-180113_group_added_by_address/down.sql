-- As SQLite does not support ALTER, we play this game of move, repopulate, drop.    Here we recreate without the 'added_by_address' column.
BEGIN TRANSACTION;
CREATE TEMPORARY TABLE backup_group(id BLOB PRIMARY KEY NOT NULL, created_at_ns BIGINT NOT NULL, membership_state INT NOT NULL, installations_last_checked BIGINT NOT NULL, purpose INT NOT NULL DEFAULT 1);
INSERT INTO backup_group SELECT id, created_at_ns, membership_state, installations_last_checked, pupose FROM groups;
DROP TABLE groups;
CREATE TABLE groups(id BLOB PRIMARY KEY NOT NULL, created_at_ns BIGINT NOT NULL, membership_state INT NOT NULL, installations_last_checked BIGINT NOT NULL, purpose INT NOT NULL DEFAULT 1);
INSERT INTO groups SELECT id, created_at_ns, membership_state, installations_last_checked, purpose FROM backup_group;
DROP TABLE backup_group;
COMMIT;
