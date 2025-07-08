DROP TABLE local_commit_log;
CREATE TABLE local_commit_log (
    -- A locally assigned ID for the local log entry
    "rowid" INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    "group_id" BLOB NOT NULL,
    -- The sequence ID of the commit being applied
    -- For welcomes, this is the sequence ID of the commit that spawned the welcome
    -- For group creation, this is 0
    "commit_sequence_id" BIGINT NOT NULL,
    -- The encryption state of the group before the commit was applied
    -- https://www.rfc-editor.org/rfc/rfc9420.html#section-8-13
    "last_epoch_authenticator" BLOB NOT NULL,
    -- Whether the commit was successfully applied or not
    -- 1 = Applied, all other values are failures matching the protobuf
    "commit_result" INT NOT NULL,
    "error_message" TEXT,
    -- The state after the commit was applied, or the existing state otherwise
    "applied_epoch_number" BIGINT NOT NULL,
    "applied_epoch_authenticator" BLOB NOT NULL,
    -- Items below this line are for debugging purposes
    "sender_inbox_id" TEXT,
    "sender_installation_id" BLOB,
    "commit_type" TEXT
);

DROP TABLE remote_commit_log;
CREATE TABLE remote_commit_log (
    -- The sequence ID of the log entry on the server
    "log_sequence_id" BIGINT NOT NULL,
    "group_id" BLOB NOT NULL,
    -- The sequence ID of the commit being referenced
    "commit_sequence_id" BIGINT NOT NULL,
    -- Whether the commit was successfully applied or not
    -- 1 = Applied, all other values are failures matching the protobuf
    "commit_result" INT NOT NULL,
    -- The state after the commit was applied, or the existing state otherwise
    "applied_epoch_number" BIGINT NOT NULL,
    "applied_epoch_authenticator" BLOB NOT NULL
);
