-- A counter that changes whenever the stored group set changes: a group row is
-- added, removed or changes a column that group discovery selects on, a
-- conversation's consent changes, or a Welcome discovery is recorded. Readers
-- that rediscover groups skip the reread while it is unchanged. The triggers
-- run in the writer's transaction, so every write path bumps it.
--
-- A later migration that rebuilds groups, consent_records or
-- group_welcome_discovery must recreate these triggers.
CREATE TABLE group_set_generation (
    id INTEGER PRIMARY KEY NOT NULL CHECK (id = 1),
    generation BIGINT NOT NULL
);

INSERT INTO group_set_generation (id, generation) VALUES (1, 0);

CREATE TRIGGER group_set_group_inserted AFTER INSERT ON groups
BEGIN
    UPDATE group_set_generation SET generation = generation + 1 WHERE id = 1;
END;

CREATE TRIGGER group_set_group_deleted AFTER DELETE ON groups
BEGIN
    UPDATE group_set_generation SET generation = generation + 1 WHERE id = 1;
END;

-- sequence_id is the group's Welcome cursor; a rejoin changes it.
CREATE TRIGGER group_set_group_changed
AFTER UPDATE OF id, membership_state, sequence_id, dm_id, conversation_type ON groups
WHEN OLD.id IS NOT NEW.id
    OR OLD.membership_state IS NOT NEW.membership_state
    OR OLD.sequence_id IS NOT NEW.sequence_id
    OR OLD.dm_id IS NOT NEW.dm_id
    OR OLD.conversation_type IS NOT NEW.conversation_type
BEGIN
    UPDATE group_set_generation SET generation = generation + 1 WHERE id = 1;
END;

-- entity_type 1 is a conversation's own consent; inbox consent does not bump.
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

CREATE TRIGGER group_set_discovery_inserted AFTER INSERT ON group_welcome_discovery
BEGIN
    UPDATE group_set_generation SET generation = generation + 1 WHERE id = 1;
END;
