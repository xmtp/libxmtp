-- Values are: 1 = Published, 2 = Unpublished
ALTER TABLE group_messages
ADD COLUMN "delivery_status" INT NOT NULL DEFAULT 1 
