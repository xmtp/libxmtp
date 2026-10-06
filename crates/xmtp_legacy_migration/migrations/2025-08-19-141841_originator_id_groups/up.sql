ALTER TABLE groups DROP COLUMN sequence_id;
ALTER TABLE groups RENAME COLUMN welcome_id TO sequence_id;

UPDATE groups SET originator_id = 11 WHERE sequence_id IS NOT NULL; -- 11 is originator for v3 welcomes
