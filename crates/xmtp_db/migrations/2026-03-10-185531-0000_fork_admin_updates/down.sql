ALTER TABLE groups DROP COLUMN fork_admin;
ALTER TABLE groups DROP COLUMN fork_admin_change_sequence_id;
ALTER TABLE remote_commit_log DROP COLUMN installation_id;
ALTER TABLE local_commit_log ADD COLUMN last_epoch_authenticator BLOB NOT NULL;
