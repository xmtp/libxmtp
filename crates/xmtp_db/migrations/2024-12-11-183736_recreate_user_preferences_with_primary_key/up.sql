DROP TABLE user_preferences;

CREATE TABLE user_preferences(
  id INTEGER PRIMARY KEY ASC NOT NULL,
  hmac_key BLOB
);
