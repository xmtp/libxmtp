CREATE TABLE IF NOT EXISTS key_value_store (
  key TEXT NOT NULL,
  value BLOB NOT NULL,
  PRIMARY KEY (key)
);
