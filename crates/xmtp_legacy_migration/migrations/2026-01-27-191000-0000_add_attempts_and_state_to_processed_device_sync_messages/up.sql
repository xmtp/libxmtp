ALTER TABLE processed_device_sync_messages
ADD COLUMN attempts INTEGER NOT NULL DEFAULT 0;

ALTER TABLE processed_device_sync_messages
ADD COLUMN state INTEGER NOT NULL DEFAULT 0;
