CREATE TABLE "fork_tester"(
    "group_id" BLOB NOT NULL,
    "fork_next_commit" BOOLEAN NOT NULL,
    PRIMARY KEY (group_id)
);