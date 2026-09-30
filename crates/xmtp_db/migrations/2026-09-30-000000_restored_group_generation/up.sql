-- Counts each change of a group into the Restored state, by any client that
-- shares this database. A receive controller reads it on each pass and checks
-- its selected groups again only after it changes.
CREATE TABLE restored_group_generation (
    id INTEGER PRIMARY KEY NOT NULL DEFAULT 0 CHECK (id = 0),
    generation BIGINT NOT NULL DEFAULT 0
);
INSERT INTO restored_group_generation (id, generation) VALUES (0, 0);

CREATE TRIGGER restored_group_inserted AFTER INSERT ON groups
WHEN NEW.membership_state = 4
BEGIN
    UPDATE restored_group_generation SET generation = generation + 1 WHERE id = 0;
END;

CREATE TRIGGER restored_group_updated AFTER UPDATE OF membership_state ON groups
WHEN NEW.membership_state = 4 AND OLD.membership_state IS NOT 4
BEGIN
    UPDATE restored_group_generation SET generation = generation + 1 WHERE id = 0;
END;
