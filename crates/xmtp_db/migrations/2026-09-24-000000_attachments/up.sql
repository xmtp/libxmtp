CREATE TABLE local_attachments (
  path          TEXT PRIMARY KEY NOT NULL,   -- "{attachment_key}/{local file name}"
  created_at_ns BIGINT NOT NULL,
  mime_type     TEXT,
  filename      TEXT
);
CREATE TABLE pending_attachments (
  content_digest    TEXT PRIMARY KEY NOT NULL, -- lowercase hex
  remote_attachment BLOB NOT NULL,             -- prost-encoded RemoteAttachmentInfo
  created_at_ns     BIGINT NOT NULL
);
CREATE INDEX pending_attachments_created_at ON pending_attachments (created_at_ns);
