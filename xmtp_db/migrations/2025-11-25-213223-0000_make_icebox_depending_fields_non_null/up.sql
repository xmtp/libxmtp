-- Migrate icebox to use a separate dependencies table
-- This allows each envelope to have multiple dependencies
-- Also adds group_id to track which group each envelope belongs to

-- Step 0: Clear existing icebox data (required for adding group_id)
DELETE FROM icebox;

-- Step 1: Create the new icebox_dependencies table
CREATE TABLE icebox_dependencies (
    envelope_sequence_id BIGINT NOT NULL,
    envelope_originator_id BIGINT NOT NULL,
    dependency_sequence_id BIGINT NOT NULL,
    dependency_originator_id BIGINT NOT NULL,
    PRIMARY KEY (envelope_originator_id, envelope_sequence_id, dependency_originator_id, dependency_sequence_id),
    -- when an envelope is deleted, also delete its dependency records
    FOREIGN KEY (envelope_originator_id, envelope_sequence_id) REFERENCES icebox(originator_id, sequence_id) ON DELETE CASCADE
);

CREATE INDEX idx_icebox_deps_lookup ON icebox_dependencies (dependency_originator_id, dependency_sequence_id);

-- Step 3: Drop the old icebox table
DROP TABLE icebox;

-- Step 4: Create new icebox table with group_id
CREATE TABLE icebox (
    sequence_id BIGINT NOT NULL,
    originator_id BIGINT NOT NULL,
    group_id BLOB NOT NULL,
    envelope_payload BLOB NOT NULL,
    PRIMARY KEY (originator_id, sequence_id),
    FOREIGN KEY (group_id) REFERENCES groups(id)
);

-- Step 5: Create index on group_id for efficient lookups
CREATE INDEX idx_icebox_group_id ON icebox (group_id);
