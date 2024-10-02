-- Your SQL goes here
ALTER TABLE groups ADD COLUMN dm_inbox_id text;
CREATE INDEX idx_dm_target ON groups(dm_inbox_id);
