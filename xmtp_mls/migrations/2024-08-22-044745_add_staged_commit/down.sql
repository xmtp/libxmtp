-- This file should undo anything in `up.sql`
ALTER TABLE group_intents
    DROP COLUMN staged_commit;

ALTER TABLE group_intents
    DROP COLUMN published_in_epoch;

