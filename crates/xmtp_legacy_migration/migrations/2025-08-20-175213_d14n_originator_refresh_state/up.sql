-- SQLite requires rebuilding the table to change primary key
CREATE TABLE refresh_state_temp AS SELECT * FROM refresh_state;

ALTER TABLE refresh_state_temp ADD COLUMN originator_id INTEGER NOT NULL DEFAULT 0;

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

-- Insert Group entries - create both ApplicationMessage and CommitMessage entries
-- from previous cursor value of `Group`
-- ApplicationMessage and CommitMessage entries.
-- - We won't skip any messages (might re-fetch some that were already synced)
-- - Migration is O(N)
-- - After migration, the next sync will update cursors to correct values
-- may get some duplicate messages in sync that will be rejected

-- Create ApplicationMessage entries
INSERT INTO refresh_state (entity_id, entity_kind, sequence_id, originator_id)
SELECT DISTINCT
    rst.entity_id,
    2 as entity_kind,  -- ApplicationMessage
    rst.cursor as sequence_id,
    10 as originator_id
FROM refresh_state_temp rst
WHERE rst.entity_kind = 2;

-- Create CommitMessage entries
INSERT INTO refresh_state (entity_id, entity_kind, sequence_id, originator_id)
SELECT DISTINCT
    rst.entity_id,
    7 as entity_kind,  -- CommitMessage
    rst.cursor as sequence_id,
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
