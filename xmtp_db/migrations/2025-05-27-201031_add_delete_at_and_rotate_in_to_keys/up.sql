-- Add a column to schedule deletion of key packages
ALTER TABLE key_package_history
    ADD COLUMN delete_in_ns BIGINT;

-- Add a column to schedule rotation of key packages
ALTER TABLE identity
    ADD COLUMN next_key_package_rotation_ns BIGINT;