CREATE TABLE restored_group_metadata (
    group_id BLOB PRIMARY KEY NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
    group_save BLOB NOT NULL
);
