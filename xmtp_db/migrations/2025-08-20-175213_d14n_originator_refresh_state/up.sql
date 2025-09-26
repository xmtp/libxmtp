-- SQLite requires rebuilding the table to change primary key
-- Step 1: Create temporary table with existing data
CREATE TABLE refresh_state_temp AS SELECT * FROM refresh_state;

-- Step 2: Add originator_id column to temporary data with proper values
ALTER TABLE refresh_state_temp ADD COLUMN originator_id INTEGER NOT NULL DEFAULT 0;

UPDATE refresh_state_temp
SET originator_id = CASE
    WHEN entity_kind = 1 THEN 11
    WHEN entity_kind = 2 THEN 0
    ELSE 0
END;

-- Step 3: Drop the original table
DROP TABLE refresh_state;

-- Step 4: Create new table with originator_id as part of primary key
CREATE TABLE refresh_state (
    entity_id BLOB NOT NULL,
    entity_kind INTEGER NOT NULL,
    cursor BIGINT NOT NULL,
    originator_id INTEGER NOT NULL,
    PRIMARY KEY (entity_id, entity_kind, originator_id)
);

-- Step 5: Restore data from temporary table
INSERT INTO refresh_state (entity_id, entity_kind, cursor, originator_id)
SELECT entity_id, entity_kind, cursor, originator_id FROM refresh_state_temp;

-- Step 6: Clean up temporary table
DROP TABLE refresh_state_temp;
