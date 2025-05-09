ALTER TABLE groups ADD COLUMN sequence_id bigint;
ALTER TABLE groups ADD COLUMN originator_id bigint;

ALTER TABLE group_messages ADD COLUMN sequence_id bigint;
ALTER TABLE group_messages ADD COLUMN originator_id bigint;
