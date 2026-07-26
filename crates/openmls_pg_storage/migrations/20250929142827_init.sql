CREATE TABLE openmls_group_data (
    group_id BYTEA NOT NULL,
    data_type TEXT NOT NULL,
    group_data BYTEA NOT NULL,
    PRIMARY KEY (group_id, data_type),
    CONSTRAINT openmls_group_data_data_type_check CHECK (
        data_type IN (
            'join_group_config',
            'tree',
            'interim_transcript_hash',
            'context',
            'confirmation_tag',
            'group_state',
            'message_secrets',
            'resumption_psk_store',
            'own_leaf_index',
            'use_ratchet_tree_extension',
            'group_epoch_secrets'
        )
    )
);

CREATE TABLE openmls_proposal (
    group_id BYTEA NOT NULL,
    proposal_ref BYTEA NOT NULL,
    proposal BYTEA NOT NULL,
    PRIMARY KEY (group_id, proposal_ref)
);

CREATE TABLE openmls_own_leaf_node (
    group_id BYTEA PRIMARY KEY,
    leaf_node BYTEA NOT NULL
);

CREATE TABLE openmls_signature_key (
    public_key BYTEA PRIMARY KEY,
    signature_key BYTEA NOT NULL
);

CREATE TABLE openmls_encryption_key (
    public_key BYTEA PRIMARY KEY,
    key_pair BYTEA NOT NULL
);

CREATE TABLE openmls_epoch_key_pairs (
    group_id BYTEA NOT NULL,
    epoch_id BYTEA NOT NULL,
    leaf_index BIGINT NOT NULL,
    key_pairs BYTEA NOT NULL,
    PRIMARY KEY (group_id, epoch_id, leaf_index)
);

CREATE TABLE openmls_key_package (
    key_package_ref BYTEA PRIMARY KEY,
    key_package BYTEA NOT NULL
);

CREATE TABLE openmls_psk (psk_id BYTEA PRIMARY KEY, psk_bundle BYTEA NOT NULL);
