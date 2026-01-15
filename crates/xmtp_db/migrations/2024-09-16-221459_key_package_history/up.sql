CREATE TABLE key_package_history (
    "id" integer PRIMARY KEY AUTOINCREMENT NOT NULL,
    "key_package_hash_ref" BLOB UNIQUE NOT NULL,
    "created_at_ns" bigint NOT NULL
);
