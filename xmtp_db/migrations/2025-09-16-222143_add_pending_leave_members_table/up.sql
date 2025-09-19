CREATE TABLE IF NOT EXISTS pending_remove(
group_id BLOB NOT NULL,
inbox_id text NOT NULL,
PRIMARY KEY (inbox_id, group_id));