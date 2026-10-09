-- Caches the computed association state at a given sequence ID in an inbox log,
-- so that we don't need to replay the whole log.
CREATE TABLE association_state (
    "inbox_id" TEXT NOT NULL,
    "sequence_id" BIGINT NOT NULL,
    "state" BLOB NOT NULL,
    PRIMARY KEY ("inbox_id", "sequence_id")
);
