DELETE FROM remote_commit_log;
DELETE FROM refresh_state WHERE entity_kind = 6; -- CommitLogForkCheckRemote
ALTER TABLE remote_commit_log ADD COLUMN author_installation_key BLOB NOT NULL DEFAULT x'';
