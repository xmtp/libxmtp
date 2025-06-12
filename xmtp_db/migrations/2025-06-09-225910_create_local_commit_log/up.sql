-- TODO: Note that commit hash could show up multiple times in the commit log.
CREATE TABLE remote_commit_log (
    "sequence_id" BIGINT NOT NULL PRIMARY KEY,
    "group_id" BLOB NOT NULL,
    "last_epoch_authenticator" BLOB,
    -- Comes from the epoch_authentication secret
    -- https://www.rfc-editor.org/rfc/rfc9420.html#section-8-13
    "epoch_authenticator" BLOB NOT NULL,
    -- 1 = Success, all other values are failures matching the protobuf
    "result" INT NOT NULL,
    -- Items below this line should be null unless result is success
    "epoch_number" BIGINT
);

CREATE TABLE local_commit_log (
    "sequence_id" BIGINT NOT NULL PRIMARY KEY,
    "group_id" BLOB NOT NULL,
    "epoch_authenticator" BLOB NOT NULL,
    -- 1 = Success, all other values are failures matching the protobuf
    "result" INT NOT NULL,
    -- Items below this line are for debugging purposes
    "epoch_number" BIGINT NOT NULL,
    "sender_inbox_id" TEXT,
    "sender_installation_id" BLOB
);
