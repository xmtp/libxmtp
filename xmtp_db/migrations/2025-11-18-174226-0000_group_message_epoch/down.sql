-- This file should undo anything in `up.sql`
ALTER TABLE group_messages DROP COLUMN published_in_epoch;
