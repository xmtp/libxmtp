DROP TABLE user_preferences;

CREATE TABLE user_preferences(
  id INTEGER PRIMARY KEY ASC,
  hmac_key BLOB NOT NULL
);
