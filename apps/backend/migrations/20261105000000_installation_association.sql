-- The inbox that first added each installation. Admission refuses a key that
-- another inbox added before it stores the update; this primary key is the
-- backstop when that check is wrong. A revocation never removes a row.
CREATE TABLE installation_association (
    installation_key bytea PRIMARY KEY CHECK (octet_length(installation_key) = 32),
    inbox_id bytea NOT NULL CHECK (octet_length(inbox_id) = 32)
);
