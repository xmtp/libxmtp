UPDATE groups SET originator_id = NULL WHERE sequence_id IS NOT NULL;
ALTER TABLE groups RENAME COLUMN sequence_id TO welcome_id;
ALTER TABLE groups ADD COLUMN sequence_id BIGINT;

