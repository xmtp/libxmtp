CREATE TABLE d14n_migration_cutover (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    cutover_ns BIGINT NOT NULL DEFAULT 9223372036854775807,
    last_checked_ns BIGINT NOT NULL DEFAULT 0,
    has_migrated BOOL NOT NULL DEFAULT FALSE
);

INSERT INTO d14n_migration_cutover (id, cutover_ns, last_checked_ns, has_migrated)
VALUES (1, 9223372036854775807, 0, FALSE);
