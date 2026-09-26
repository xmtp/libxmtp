CREATE TABLE local_attachments (
  path          TEXT PRIMARY KEY NOT NULL,   -- "{attachment_key}/{local file name}"
  created_at_ns BIGINT NOT NULL,
  mime_type     TEXT,
  filename      TEXT
);
CREATE TABLE pending_attachments (
  content_digest          TEXT PRIMARY KEY NOT NULL, -- lowercase hex
  remote_attachment       BLOB NOT NULL,             -- prost-encoded RemoteAttachmentInfo
  created_at_ns           BIGINT NOT NULL,
  status                  TEXT NOT NULL DEFAULT 'waiting'
                          CHECK (status IN ('waiting', 'uploading', 'complete', 'failed')),
  failure_cause           TEXT,
  failure_credential_kind TEXT,
  failure_retryable       BOOLEAN,
  failure_missing_scope   BOOLEAN,
  failure_http_status     INTEGER CHECK (failure_http_status BETWEEN 100 AND 999),
  lease_id                BLOB,
  lease_expires_at_ns     BIGINT,
  CHECK ((status = 'uploading') = (lease_id IS NOT NULL AND lease_expires_at_ns IS NOT NULL))
);
CREATE INDEX pending_attachments_created_at ON pending_attachments (created_at_ns);
