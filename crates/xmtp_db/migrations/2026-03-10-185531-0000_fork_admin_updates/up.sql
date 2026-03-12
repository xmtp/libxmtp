ALTER TABLE groups ADD COLUMN fork_admin BLOB;
ALTER TABLE groups ADD COLUMN fork_admin_change_sequence_id BIGINT;
DELETE FROM local_commit_log;
DELETE FROM remote_commit_log;
ALTER TABLE remote_commit_log ADD COLUMN installation_id BLOB NOT NULL;
ALTER TABLE local_commit_log DROP COLUMN last_epoch_authenticator;
