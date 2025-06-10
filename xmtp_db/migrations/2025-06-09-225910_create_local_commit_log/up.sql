-- TODO: Note that commit hash could show up multiple times in the commit log.

CREATE TABLE "remote_commit_log"(
    "last_state_hash" INT,
    "group_id" BLOB NOT NULL,
    "commit_hash" INT NOT NULL,
    -- 1 = Success, all other values are failures matching the protobuf
    "result" INT NOT NULL,
    -- Items below this line should be null unless result is success
    "state_hash" INT,
    "epoch_number" BIGINT NOT NULL,
)

CREATE TABLE "local_commit_log"(
    "group_id" BLOB NOT NULL,
    "commit_hash" INT NOT NULL,
    -- 1 = Success, all other values are failures matching the protobuf
    "result" INT NOT NULL,
    -- Items below this line should be null unless the payload was decryptable
    "state_hash" INT,
    "epoch_number" BIGINT NOT NULL, -- For debugging purposes
    "sender_inbox_id" TEXT, -- For debugging purposes
    "sender_installation_id" BLOB, -- For debugging purposes
);

