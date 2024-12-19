ALTER TABLE group_messages
    ADD COLUMN content_type INTEGER NOT NULL DEFAULT 0;

ALTER TABLE group_messages
    ADD COLUMN version_minor INTEGER NOT NULL DEFAULT 0;

ALTER TABLE group_messages
    ADD COLUMN version_major INTEGER NOT NULL DEFAULT 0;

ALTER TABLE group_messages
    ADD COLUMN authority_id TEXT NOT NULL DEFAULT '';
