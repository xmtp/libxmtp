DROP TRIGGER group_set_consent_inserted;
DROP TRIGGER group_set_consent_changed;
DROP TRIGGER group_set_consent_deleted;

CREATE TRIGGER group_set_consent_inserted AFTER INSERT ON consent_records
WHEN NEW.entity_type = 1
BEGIN
    UPDATE group_set_generation SET generation = generation + 1 WHERE id = 1;
END;

CREATE TRIGGER group_set_consent_changed
AFTER UPDATE OF entity_type, entity, state ON consent_records
WHEN (OLD.entity_type = 1 OR NEW.entity_type = 1)
    AND (OLD.entity_type IS NOT NEW.entity_type
        OR OLD.entity IS NOT NEW.entity
        OR OLD.state IS NOT NEW.state)
BEGIN
    UPDATE group_set_generation SET generation = generation + 1 WHERE id = 1;
END;

CREATE TRIGGER group_set_consent_deleted AFTER DELETE ON consent_records
WHEN OLD.entity_type = 1
BEGIN
    UPDATE group_set_generation SET generation = generation + 1 WHERE id = 1;
END;

ALTER TABLE user_preferences DROP COLUMN owed_counter;
ALTER TABLE user_preferences DROP COLUMN hmac_key_owed_seq;
ALTER TABLE consent_records DROP COLUMN owed_seq;
