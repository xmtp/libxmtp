CREATE TABLE association_state (
    "inbox_id" TEXT NOT NULL,
    "sequence_id" BIGINT NOT NULL,
    "state" BLOB NOT NULL,
    PRIMARY KEY ("inbox_id", "sequence_id")
);
