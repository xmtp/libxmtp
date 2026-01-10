create table group_locks (
  group_id BLOB PRIMARY KEY NOT NULL,
  locked_at_ns BIGINT NOT NULL,
  expires_at_ns BIGINT NOT NULL
);