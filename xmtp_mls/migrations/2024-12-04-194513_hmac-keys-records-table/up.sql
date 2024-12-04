CREATE TABLE "hmac_key_records"(
    -- Group ID that the Hmac keys are associated with
    "group_id" BLOB NOT NULL,
    -- Dm ID that the Hmac keys are associated with 
    "dm_id" TEXT,
    -- The hmac key
    "hmac_key" BLOB NOT NULL,
    -- The number of 30 day periods since epoch
    "thirty_day_periods_since_epoch" INT NOT NULL,
    PRIMARY KEY ("group_id", "hmac_key")
);
