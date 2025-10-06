-- SQLite requires rebuilding the table to change primary key
-- Step 1: Create temporary table with existing data
CREATE TABLE refresh_state_temp AS SELECT * FROM refresh_state;

-- Step 2: Add originator_id column to temporary data
ALTER TABLE refresh_state_temp ADD COLUMN originator_id INTEGER NOT NULL DEFAULT 0;

-- Step 3: Set originator_id based on entity_kind and correlation with group_messages
UPDATE refresh_state_temp
SET originator_id = CASE
    WHEN entity_kind = 1 THEN 11 -- Welcome
    WHEN entity_kind = 2 THEN (
        -- For Group entries, determine originator by matching sequence_id with group_messages
        SELECT CASE
            WHEN gm.kind = 1 THEN 10 -- Application message
            WHEN gm.kind = 2 THEN 0  -- Commit (MembershipChange)
            ELSE 0 -- Default to commit message if no match
        END
        FROM group_messages gm
        WHERE gm.group_id = refresh_state_temp.entity_id
            AND gm.sequence_id = refresh_state_temp.cursor
        LIMIT 1
    )
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

-- Step 7: Insert Group entries that matched application messages (kind = 1, originator 10)
-- Keep entity_kind = 2 (Group)
INSERT INTO refresh_state (entity_id, entity_kind, sequence_id, originator_id)
SELECT entity_id, entity_kind, cursor, originator_id
FROM refresh_state_temp
WHERE entity_kind = 2 AND originator_id = 10;

-- Step 8: Insert Group entries that matched commits (kind = 2, originator 0)
-- Create as entity_kind = 7 (CommitMessage)
INSERT INTO refresh_state (entity_id, entity_kind, sequence_id, originator_id)
SELECT entity_id, 7 as entity_kind, cursor, originator_id
FROM refresh_state_temp
WHERE entity_kind = 2 AND originator_id = 0;

-- Step 9: Insert all non-Group entries (Welcome, CommitLog, etc.)
INSERT INTO refresh_state (entity_id, entity_kind, sequence_id, originator_id)
SELECT entity_id, entity_kind, cursor, originator_id
FROM refresh_state_temp
WHERE entity_kind != 2 AND entity_kind != 7;

-- Step 10: Clean up temporary table
DROP TABLE refresh_state_temp;
