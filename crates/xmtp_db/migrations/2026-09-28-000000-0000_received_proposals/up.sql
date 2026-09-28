CREATE TABLE received_proposals (
    group_id BLOB NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
    epoch BIGINT NOT NULL CHECK (epoch >= 0),
    proposal_ref BLOB NOT NULL,
    PRIMARY KEY (group_id, epoch, proposal_ref)
) WITHOUT ROWID;
