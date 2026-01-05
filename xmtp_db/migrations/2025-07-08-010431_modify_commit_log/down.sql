DROP TABLE remote_commit_log;
CREATE TABLE remote_commit_log (
    "log_sequence_id" BIGINT NOT NULL,
    "group_id" BLOB NOT NULL,
    "commit_sequence_id" BIGINT NOT NULL,
    "commit_result" INT NOT NULL,
    "applied_epoch_number" BIGINT,
    "applied_epoch_authenticator" BLOB
);

DROP TABLE local_commit_log;
CREATE TABLE local_commit_log (
    "rowid" INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    "group_id" BLOB NOT NULL,
    "commit_sequence_id" BIGINT NOT NULL,
    "last_epoch_authenticator" BLOB NOT NULL,
    "commit_result" INT NOT NULL,
    "applied_epoch_number" BIGINT,
    "applied_epoch_authenticator" BLOB,
    "sender_inbox_id" TEXT,
    "sender_installation_id" BLOB,
    "commit_type" INT
);
