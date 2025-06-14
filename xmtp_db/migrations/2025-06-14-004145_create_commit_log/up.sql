CREATE TABLE local_commit_log (
    -- A locally assigned ID for the local log entry
    "log_id" INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    "group_id" BLOB NOT NULL,
    -- The sequence ID of the commit being applied
    "commit_sequence_id" BIGINT NOT NULL,
    -- The encryption state of the group before the commit was applied
    -- https://www.rfc-editor.org/rfc/rfc9420.html#section-8-13
    "last_epoch_authenticator" BLOB NOT NULL,
    -- Whether the commit was successfully applied or not
    -- 1 = Applied, all other values are failures matching the protobuf
    "commit_result" INT NOT NULL,
    -- Items below this line are only set if the commit was applied
    "applied_epoch_number" BIGINT,
    "applied_epoch_authenticator" BLOB,
    -- Items below this line are for debugging purposes
    "sender_inbox_id" TEXT,
    "sender_installation_id" BLOB,
    "commit_type" INT
);

CREATE TABLE remote_commit_log (
    -- The sequence ID of the log entry on the server
    "log_sequence_id" INTEGER PRIMARY KEY NOT NULL,
    "group_id" BLOB NOT NULL,
    -- The sequence ID of the commit being referenced
    "commit_sequence_id" BIGINT NOT NULL,
    -- Whether the commit was successfully applied or not
    -- 1 = Applied, all other values are failures matching the protobuf
    "commit_result" INT NOT NULL,
    -- Items below this line are only set if the commit was succssfully decrypted
    "applied_epoch_number" BIGINT,
    "applied_epoch_authenticator" BLOB
);
