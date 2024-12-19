ALTER TABLE group_messages
    DROP COLUMN authority_id;

ALTER TABLE group_messages
    DROP COLUMN version_major;

ALTER TABLE group_messages
    DROP COLUMN version_minor;

ALTER TABLE group_messages
    DROP COLUMN content_type;
