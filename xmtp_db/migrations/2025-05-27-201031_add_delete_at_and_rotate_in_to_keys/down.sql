ALTER TABLE key_package_history
DROP COLUMN delete_in_ns;

ALTER TABLE identity
DROP COLUMN next_key_package_rotation_ns;