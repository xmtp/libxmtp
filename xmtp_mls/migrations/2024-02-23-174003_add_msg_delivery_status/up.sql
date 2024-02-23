ALTER TABLE group_messages
ADD COLUMN "delivery_status" TEXT NOT NULL DEFAULT 'PUBLISHED'
CHECK ("delivery_status" IN ('PUBLISHED', 'UNPUBLISHED'));

