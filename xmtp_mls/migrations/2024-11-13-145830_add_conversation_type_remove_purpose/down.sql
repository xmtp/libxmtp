ALTER TABLE groups
ADD COLUMN purpose INTEGER NOT NULL;

UPDATE groups
SET purpose = CASE
    WHEN conversation_type = 3 THEN 2
    ELSE 1
END;

ALTER TABLE groups DROP COLUMN conversation_type;
