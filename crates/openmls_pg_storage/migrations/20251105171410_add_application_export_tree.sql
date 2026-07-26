-- Migration: Add application_export_tree to the openmls_group_data CHECK.
--
-- Postgres can alter a CHECK constraint in place, so unlike the SQLite source
-- (which rebuilds the whole table) this just swaps the named constraint. The
-- end state is identical: 'application_export_tree' becomes a permitted value.
ALTER TABLE openmls_group_data
    DROP CONSTRAINT openmls_group_data_data_type_check;

ALTER TABLE openmls_group_data
    ADD CONSTRAINT openmls_group_data_data_type_check CHECK (
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
            'group_epoch_secrets',
            'application_export_tree'
        )
    );
