ALTER TABLE groups
ADD COLUMN conversation_type INTEGER NOT NULL;

UPDATE groups
SET conversation_type = CASE
    -- Purpose is conversation and is not a DM
    -- Then set to 1 (ConversationType::Group)
    WHEN purpose = 1 AND dm_inbox_id IS NULL THEN 1
    -- Otherwise dm_inbox_id is not null
    -- Then set to 2 (ConversationType::Dm)
    WHEN purpose = 1 THEN 2
    -- Otherwise this is a Sync Group
    ELSE 3
END;

ALTER TABLE groups DROP COLUMN purpose;
