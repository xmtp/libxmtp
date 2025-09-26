-- SQLite requires rebuilding the table to revert primary key change
-- Step 1: Create temporary table with existing data (excluding originator_id from primary key)
CREATE TABLE refresh_state_temp AS SELECT entity_id, entity_kind, cursor FROM refresh_state;

-- Step 2: Drop the table with compound primary key
DROP TABLE refresh_state;

-- Step 3: Recreate original table structure with original primary key
CREATE TABLE refresh_state (
    entity_id BLOB NOT NULL,
    entity_kind INTEGER NOT NULL,
    cursor BIGINT NOT NULL,
    PRIMARY KEY (entity_id, entity_kind)
);

-- Step 4: Restore data (this will naturally deduplicate based on the new primary key)
INSERT OR REPLACE INTO refresh_state (entity_id, entity_kind, cursor)
SELECT entity_id, entity_kind, cursor FROM refresh_state_temp;

-- Step 5: Clean up temporary table
DROP TABLE refresh_state_temp;
