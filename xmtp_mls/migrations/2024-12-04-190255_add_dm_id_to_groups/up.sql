ALTER TABLE groups
ADD COLUMN dm_id TEXT;

UPDATE groups
SET dm_id = LOWER(CONCAT('dm:', dm_inbox_id))
WHERE dm_inbox_id IS NOT NULL;