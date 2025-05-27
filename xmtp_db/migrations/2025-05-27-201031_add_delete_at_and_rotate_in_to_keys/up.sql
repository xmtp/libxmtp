-- Add a column to schedule deletion of key packages
ALTER TABLE key_package_history
    ADD COLUMN delete_in BIGINT;

-- Add a column to schedule rotation of key packages
ALTER TABLE key_package_history
    ADD COLUMN rotate_in BIGINT;