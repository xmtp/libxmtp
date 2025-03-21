CREATE TABLE "user_preferences"(
    -- The latest id is the current preference
    id INTEGER PRIMARY KEY ASC,
    -- HMAC root key
    hmac_key BLOB
);
