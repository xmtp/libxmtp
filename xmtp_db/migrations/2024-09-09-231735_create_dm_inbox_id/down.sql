-- This file should undo anything in `up.sql`
DROP INDEX idx_dm_target;
ALTER TABLE groups DROP COLUMN dm_inbox_id;
