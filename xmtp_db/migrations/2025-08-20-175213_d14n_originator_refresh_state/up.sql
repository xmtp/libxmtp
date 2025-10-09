-- SQLite requires rebuilding the table to change primary key
-- Step 1: Create temporary table with existing data
CREATE TABLE refresh_state_temp AS SELECT * FROM refresh_state;

-- Step 2: Add originator_id column to temporary data
ALTER TABLE refresh_state_temp ADD COLUMN originator_id INTEGER NOT NULL DEFAULT 0;

-- Step 3: Set originator_id based on entity_kind
-- For non-Group entries (Welcome, CommitLog, etc.), set originator_id directly
UPDATE refresh_state_temp
SET originator_id = CASE
    WHEN entity_kind = 1 THEN 11 -- Welcome
    WHEN entity_kind = 2 THEN 0  -- Group entries will be handled specially in insert
    ELSE 100 -- everything else (CommitLog)
END;

DROP TABLE refresh_state;

CREATE TABLE refresh_state (
    entity_id BLOB NOT NULL,
    entity_kind INTEGER NOT NULL,
    sequence_id BIGINT NOT NULL CHECK (sequence_id >= 0) ,
    originator_id INTEGER NOT NULL CHECK (originator_id >= 0),
    PRIMARY KEY (entity_id, entity_kind, originator_id)
);

-- Step 7: Insert Group entries - create both ApplicationMessage and CommitMessage entries
-- IMPORTANT CONSTRAINT: The cursor in refresh_state is only updated during sync operations.
-- - Commit messages (kind=2) are ALWAYS from sync, so their sequence_ids are trustworthy
-- - Application messages (kind=1) in group_messages may be local (not synced)
--
-- Strategy:
-- 1. If the old cursor matches a commit message: use that cursor for BOTH app and commit
--    (we can't trust app messages with higher sequence_ids - they weren't synced)
-- 2. If the old cursor matches an app message OR nothing:
--    - Use the old cursor for ApplicationMessage (this was synced)
--    - Use MAX(commit sequence_ids) for CommitMessage (commits are always synced)

-- Create ApplicationMessage entries
INSERT INTO refresh_state (entity_id, entity_kind, sequence_id, originator_id)
SELECT DISTINCT
    rst.entity_id,
    2 as entity_kind,  -- ApplicationMessage
    -- If old cursor was a commit, use that cursor; otherwise use old cursor as-is
    CASE
        WHEN EXISTS (
            SELECT 1 FROM group_messages gm
            WHERE gm.group_id = rst.entity_id
              AND gm.sequence_id = rst.cursor
              AND gm.kind = 2  -- It was a commit
        ) THEN rst.cursor
        ELSE rst.cursor  -- It was an app message or nothing matched
    END as sequence_id,
    10 as originator_id
FROM refresh_state_temp rst
WHERE rst.entity_kind = 2;

-- Create CommitMessage entries
INSERT INTO refresh_state (entity_id, entity_kind, sequence_id, originator_id)
SELECT DISTINCT
    rst.entity_id,
    7 as entity_kind,  -- CommitMessage
    -- If old cursor was a commit, use that cursor
    -- Otherwise use MAX(commit sequence_ids) since commits are always synced
    CASE
        WHEN EXISTS (
            SELECT 1 FROM group_messages gm
            WHERE gm.group_id = rst.entity_id
              AND gm.sequence_id = rst.cursor
              AND gm.kind = 2  -- It was a commit
        ) THEN rst.cursor
        ELSE COALESCE(
            (SELECT MAX(gm.sequence_id)
             FROM group_messages gm
             WHERE gm.group_id = rst.entity_id AND gm.kind = 2),
            rst.cursor  -- No commits exist, use old cursor
        )
    END as sequence_id,
    0 as originator_id
FROM refresh_state_temp rst
WHERE rst.entity_kind = 2;

-- Step 8: Insert all non-Group entries (Welcome, CommitLog, etc.)
INSERT INTO refresh_state (entity_id, entity_kind, sequence_id, originator_id)
SELECT entity_id, entity_kind, cursor, originator_id
FROM refresh_state_temp
WHERE entity_kind != 2 AND entity_kind != 7;

-- Step 9: Clean up temporary table
DROP TABLE refresh_state_temp;
