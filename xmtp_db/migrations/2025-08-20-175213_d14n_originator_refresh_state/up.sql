ALTER TABLE refresh_state
ADD COLUMN originator_id INTEGER NOT NULL DEFAULT 0;

UPDATE refresh_state
SET originator_id = CASE
    WHEN entity_kind = 1 THEN 11
    WHEN entity_kind = 2 THEN 0
    ELSE 0
END;
