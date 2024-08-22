-- Your SQL goes here
ALTER TABLE group_intents
    ADD COLUMN staged_commit BLOB;

ALTER TABLE group_intents
    ADD COLUMN published_in_epoch BIGINT;

