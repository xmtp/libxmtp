CREATE TABLE refresh_jobs (
    id TEXT PRIMARY KEY NOT NULL,
    last_run BIGINT NOT NULL
);

INSERT INTO refresh_jobs
    VALUES ('invite', 0);

INSERT INTO refresh_jobs
    VALUES ('message', 0)