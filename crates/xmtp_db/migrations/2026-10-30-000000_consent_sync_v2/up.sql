-- 0 when the record is not owed, else the mark that owes it.
ALTER TABLE consent_records ADD COLUMN owed_seq BIGINT NOT NULL DEFAULT 0;
ALTER TABLE user_preferences ADD COLUMN hmac_key_owed_seq BIGINT NOT NULL DEFAULT 0;
-- The source of fresh marks.
ALTER TABLE user_preferences ADD COLUMN owed_counter BIGINT NOT NULL DEFAULT 0;

-- Inbox consent now decides DMs, so an inbox record changes the group set
-- too. These replace the entity_type = 1 triggers of
-- 2026-10-25-010000_group_set_generation. An owed_seq update alone does not
-- bump the generation.
DROP TRIGGER group_set_consent_inserted;
DROP TRIGGER group_set_consent_changed;
DROP TRIGGER group_set_consent_deleted;

CREATE TRIGGER group_set_consent_inserted AFTER INSERT ON consent_records
BEGIN
    UPDATE group_set_generation SET generation = generation + 1 WHERE id = 1;
END;

CREATE TRIGGER group_set_consent_changed
AFTER UPDATE OF entity_type, entity, state ON consent_records
WHEN OLD.entity_type IS NOT NEW.entity_type
    OR OLD.entity IS NOT NEW.entity
    OR OLD.state IS NOT NEW.state
BEGIN
    UPDATE group_set_generation SET generation = generation + 1 WHERE id = 1;
END;

CREATE TRIGGER group_set_consent_deleted AFTER DELETE ON consent_records
BEGIN
    UPDATE group_set_generation SET generation = generation + 1 WHERE id = 1;
END;
