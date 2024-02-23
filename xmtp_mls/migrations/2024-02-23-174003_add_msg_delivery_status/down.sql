-- Copy and replace is only necesasry for SQLite as SQLite does not support DROP COLUMN directly.
-- Create a new temporary table without the `delivery_status` column
CREATE TABLE tmp_group_messages AS SELECT * FROM group_messages;

-- Copy the data from the original table to the new temp table (excluding the `delivery_status` column)
INSERT INTO tmp_group_messages SELECT * FROM group_messages;

-- Drop the original `group_messages` table thereby removing the `delivery_status` column
DROP TABLE group_messages;

-- Rename the new temp table to the original name
ALTER TABLE tmp_group_messages RENAME TO group_messages;
