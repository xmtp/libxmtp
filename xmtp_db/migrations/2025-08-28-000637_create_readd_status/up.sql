CREATE TABLE readd_status (
    "group_id" BLOB NOT NULL,
    "inbox_id" TEXT NOT NULL,
    "installation_id" BLOB NOT NULL,
    "requested_at_sequence_id" BIGINT,
    "responded_at_sequence_id" BIGINT,
    PRIMARY KEY ("group_id", "inbox_id", "installation_id")
);
