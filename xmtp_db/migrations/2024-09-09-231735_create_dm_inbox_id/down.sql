-- This file should undo anything in `up.sql`
ALTER TABLE groups DROP COLUMN dm_inbox_id;
DROP INDEX idx_dm_target;
