-- SQLite requires rebuilding the table to revert primary key change
-- Step 1: Create temporary table with existing data
-- Convert CommitMessage (entity_kind=7) back to Group (entity_kind=2)
CREATE TABLE refresh_state_temp AS
SELECT
    entity_id,
    CASE
        WHEN entity_kind = 7 THEN 2 -- CommitMessage -> Group
        ELSE entity_kind
    END as entity_kind,
    sequence_id as cursor
FROM refresh_state;

-- Step 2: Drop the table with compound primary key
DROP TABLE refresh_state;

-- Step 3: Recreate original table structure with original primary key (no originator_id)
CREATE TABLE refresh_state (
    entity_id BLOB NOT NULL,
    entity_kind INTEGER NOT NULL,
    cursor BIGINT NOT NULL,
    PRIMARY KEY (entity_id, entity_kind)
);

-- Step 4: Restore data (this will deduplicate - keeping the max cursor for each entity_id/entity_kind pair)
INSERT OR REPLACE INTO refresh_state (entity_id, entity_kind, cursor)
SELECT entity_id, entity_kind, MAX(cursor)
FROM refresh_state_temp
GROUP BY entity_id, entity_kind;

-- Step 5: Clean up temporary table
DROP TABLE refresh_state_temp;
