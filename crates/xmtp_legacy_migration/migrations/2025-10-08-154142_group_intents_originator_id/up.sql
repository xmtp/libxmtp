ALTER TABLE group_intents ADD COLUMN originator_id BIGINT;

-- Set default values based on intent kind
UPDATE group_intents SET originator_id = 10 WHERE kind = 1; -- SendMessage
UPDATE group_intents SET originator_id = 0 WHERE kind = 2; -- KeyUpdate
UPDATE group_intents SET originator_id = 0 WHERE kind = 3; -- MetadataUpdate
UPDATE group_intents SET originator_id = 0 WHERE kind = 4; -- UpdateGroupMembership
UPDATE group_intents SET originator_id = 0 WHERE kind = 5; -- UpdateAdminList
UPDATE group_intents SET originator_id = 0 WHERE kind = 6; -- UpdatePermission
