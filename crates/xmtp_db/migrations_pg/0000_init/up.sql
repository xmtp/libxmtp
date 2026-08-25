-- Consolidated PostgreSQL schema.
--
-- Column names, order, and nullability mirror `schema_gen.rs` exactly; diesel does
-- not validate DDL at compile time, so any drift here is a silent runtime bug.

-- OpenMLS crypto store (async/sqlx track). These are the per-item-type tables
-- the `openmls_pg_storage` StorageProvider actually reads and writes — NOT the
-- sync/SQLite track's generic `openmls_key_value` KV table (that one belongs to
-- the diesel `sql_key_store` and has no place on Postgres). Source of truth:
-- `openmls_pg_storage/migrations/*.sql`, folded in here (incl. the later
-- `application_export_tree` CHECK addition) so a single `run_pending_migrations`
-- installs the whole async schema.
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
            'group_epoch_secrets',
            'application_export_tree'
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

-- PROTOTYPE (Postgres-only): purpose-built tables for the three KV labels that
-- are libxmtp's OWN data (not OpenMLS StorageProvider state — that goes to
-- `openmls_pg_storage`'s typed tables above). `PgKeyStore`'s read/write/delete
-- dispatch on the label to these; there is deliberately NO generic
-- `openmls_key_value` fallback — an unrecognized label panics, forcing a real
-- table + migration rather than stranding data in a backup table. This is
-- invisible outside PgKeyStore: the XmtpMlsStorageProvider trait is a
-- (label,key)->bytes interface, so the physical layout is the store's business
-- and the sync/SQLite track is unchanged. Values are stored verbatim (the
-- caller's already-serialized bytes) so `read<V>` deserializes exactly as before;
-- only the KEY becomes a real, semantic PK.
--
-- public_key -> serialized key-package hash ref. Keyed by the TLS-serialized
-- HPKE init key (or the post-quantum public key); both share this table.
CREATE TABLE kp_references (
    public_key BYTEA PRIMARY KEY,
    hash_ref BYTEA NOT NULL
);

-- key-package hash ref -> post-quantum wrapper private key.
CREATE TABLE kp_wrapper_private_keys (
    hash_ref BYTEA PRIMARY KEY,
    private_key BYTEA NOT NULL
);

-- group id -> commit-log signer private key. The label's key arrives as
-- bincode(group_id); PgKeyStore decodes it to the raw id so this is a clean,
-- join-able 16-byte group id (a FK to "groups"(id) is a natural follow-up, left
-- off for now to avoid write-ordering assumptions).
CREATE TABLE commit_log_signer_keys (
    group_id BYTEA PRIMARY KEY,
    private_key BYTEA NOT NULL
);

-- Identity of this installation. There can only be one, hence CHECK (rowid = 1).
-- SQLite uses its implicit `rowid` here; Postgres has no such pseudo-column, so
-- `rowid` is a real SERIAL column. It stays in fourth position to match diesel's
-- column order, which Postgres permits for a serial column.
CREATE TABLE identity (
    inbox_id TEXT NOT NULL,
    installation_keys BYTEA NOT NULL,
    credential_bytes BYTEA NOT NULL,
    rowid SERIAL PRIMARY KEY CHECK (rowid = 1),
    next_key_package_rotation_ns BIGINT,
    registration_cursor_originator_id BIGINT,
    registration_cursor_sequence_id BIGINT
);

CREATE TABLE identity_cache (
    inbox_id TEXT NOT NULL,
    identity TEXT NOT NULL,
    identity_kind INTEGER NOT NULL,
    PRIMARY KEY (identity, identity_kind)
);

-- Caches the identity update payload at a given sequence ID, so that API calls
-- don't need to be repeated.
CREATE TABLE identity_updates (
    inbox_id TEXT NOT NULL,
    sequence_id BIGINT NOT NULL,
    server_timestamp_ns BIGINT NOT NULL,
    payload BYTEA NOT NULL,
    originator_id INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (inbox_id, sequence_id)
);

-- Caches the computed association state at a given sequence ID in an inbox log,
-- so that we don't need to replay the whole log.
CREATE TABLE association_state (
    inbox_id TEXT NOT NULL,
    sequence_id BIGINT NOT NULL,
    state BYTEA NOT NULL,
    PRIMARY KEY (inbox_id, sequence_id)
);

CREATE TABLE consent_records (
    -- Enum of the CONSENT_TYPE (GROUP_ID, INBOX_ID, etc..)
    entity_type INTEGER NOT NULL,
    -- Enum of CONSENT_STATE (ALLOWED, DENIED, etc..)
    state INTEGER NOT NULL,
    entity TEXT NOT NULL,
    consented_at_ns BIGINT NOT NULL DEFAULT 0,
    PRIMARY KEY (entity_type, entity)
);

CREATE TABLE "groups" (
    -- Random ID generated by group creator
    id BYTEA PRIMARY KEY NOT NULL,
    -- Based on the timestamp of the welcome message
    created_at_ns BIGINT NOT NULL,
    -- Enum of GROUP_MEMBERSHIP_STATE
    membership_state INTEGER NOT NULL,
    -- Last time the installations were checked for the purpose of seeing if any are missing
    installations_last_checked BIGINT NOT NULL,
    -- Which inbox added you to the group
    added_by_inbox_id TEXT NOT NULL,
    -- Sequence ID of the welcome that created this group
    sequence_id BIGINT,
    rotated_at_ns BIGINT NOT NULL DEFAULT 0,
    conversation_type INTEGER NOT NULL DEFAULT 1,
    dm_id TEXT,
    last_message_ns BIGINT,
    message_disappear_from_ns BIGINT,
    message_disappear_in_ns BIGINT,
    paused_for_version TEXT DEFAULT NULL,
    maybe_forked BOOLEAN NOT NULL DEFAULT FALSE,
    fork_details TEXT NOT NULL DEFAULT '',
    originator_id BIGINT,
    should_publish_commit_log BOOLEAN NOT NULL DEFAULT FALSE,
    commit_log_public_key BYTEA,
    is_commit_log_forked BOOLEAN,
    has_pending_leave_request BOOLEAN
);

-- Successfully processed messages meant to be returned to the user
CREATE TABLE group_messages (
    -- Derived via generate_message_id() in SDK, which hashes several inputs
    id BYTEA PRIMARY KEY NOT NULL,
    group_id BYTEA NOT NULL,
    -- Message contents after decryption
    decrypted_message_bytes BYTEA NOT NULL,
    -- Based on the timestamp of the message
    sent_at_ns BIGINT NOT NULL,
    -- Enum GROUP_MESSAGE_KIND
    kind INTEGER NOT NULL DEFAULT 1,
    sender_installation_id BYTEA NOT NULL,
    sender_inbox_id TEXT NOT NULL,
    -- Values are: 1 = Published, 2 = Unpublished
    delivery_status INTEGER NOT NULL DEFAULT 0,
    content_type INTEGER NOT NULL DEFAULT 0,
    version_major INTEGER NOT NULL DEFAULT 0,
    version_minor INTEGER NOT NULL DEFAULT 0,
    authority_id TEXT NOT NULL,
    reference_id BYTEA,
    originator_id BIGINT NOT NULL,
    sequence_id BIGINT NOT NULL,
    -- Database-assigned insert time in nanoseconds. SQLite composes this from
    -- strftime('%s'/'%f'); clock_timestamp() is the Postgres equivalent and, like
    -- SQLite's 'now', advances within a transaction so bulk inserts stay ordered.
    inserted_at_ns BIGINT NOT NULL DEFAULT ((EXTRACT(EPOCH FROM clock_timestamp()) * 1000000000)::BIGINT),
    expire_at_ns BIGINT,
    should_push BOOLEAN NOT NULL DEFAULT TRUE,
    -- The message id is derived from this key; exposing it lets callers make
    -- application-level retries idempotent.
    idempotency_key TEXT NOT NULL DEFAULT '',
    -- Not in schema_gen.rs: message listing orders by SQLite's implicit rowid as
    -- an insertion-order tie-break. Postgres has no implicit rowid, so the column
    -- is materialized here under the same name. Diesel never selects or inserts
    -- it, so it stays invisible to the models.
    rowid BIGSERIAL NOT NULL,
    FOREIGN KEY (group_id) REFERENCES "groups" (id)
);

-- Tracks message deletions for soft-delete functionality. Deletion records are
-- removed when the DeleteMessage itself is purged from group_messages (FK CASCADE);
-- the target message is deliberately not cascade-deleted.
CREATE TABLE message_deletions (
    -- The ID of the DeleteMessage in the group_messages table
    id BYTEA PRIMARY KEY NOT NULL,
    group_id BYTEA NOT NULL,
    -- The ID of the original message being deleted
    deleted_message_id BYTEA NOT NULL,
    deleted_by_inbox_id TEXT NOT NULL,
    -- Whether the deleter was a super admin at deletion time
    is_super_admin_deletion BOOLEAN NOT NULL,
    deleted_at_ns BIGINT NOT NULL,
    FOREIGN KEY (id) REFERENCES group_messages (id) ON DELETE CASCADE
);

-- Required to retry messages that do not send successfully due to epoch conflicts
CREATE TABLE group_intents (
    id SERIAL PRIMARY KEY NOT NULL,
    -- Enum INTENT_KIND
    kind INTEGER NOT NULL,
    group_id BYTEA NOT NULL,
    -- Serializable blob used to re-try the message if the first attempt conflicted
    data BYTEA NOT NULL,
    -- INTENT_STATE
    state INTEGER NOT NULL,
    -- The hash of the encrypted, concrete form of the message if it was published
    payload_hash BYTEA UNIQUE,
    -- (Optional) data needed for the post-commit flow, e.g. welcome messages
    post_commit_data BYTEA,
    publish_attempts INTEGER NOT NULL DEFAULT 0,
    staged_commit BYTEA,
    published_in_epoch BIGINT,
    should_push BOOLEAN NOT NULL DEFAULT TRUE,
    sequence_id BIGINT,
    originator_id BIGINT,
    FOREIGN KEY (group_id) REFERENCES "groups" (id)
);

CREATE TABLE icebox (
    originator_id BIGINT NOT NULL,
    sequence_id BIGINT NOT NULL,
    group_id BYTEA NOT NULL,
    envelope_payload BYTEA NOT NULL,
    PRIMARY KEY (originator_id, sequence_id),
    FOREIGN KEY (group_id) REFERENCES "groups" (id)
);

CREATE TABLE icebox_dependencies (
    envelope_originator_id BIGINT NOT NULL,
    envelope_sequence_id BIGINT NOT NULL,
    dependency_originator_id BIGINT NOT NULL,
    dependency_sequence_id BIGINT NOT NULL,
    PRIMARY KEY (
        envelope_originator_id,
        envelope_sequence_id,
        dependency_originator_id,
        dependency_sequence_id
    ),
    -- when an envelope is deleted, also delete its dependency records
    FOREIGN KEY (envelope_originator_id, envelope_sequence_id)
        REFERENCES icebox (originator_id, sequence_id) ON DELETE CASCADE
);

-- Keeps track of the last seen cursor per topic
CREATE TABLE refresh_state (
    entity_id BYTEA NOT NULL,
    entity_kind INTEGER NOT NULL,
    sequence_id BIGINT NOT NULL CHECK (sequence_id >= 0),
    originator_id INTEGER NOT NULL CHECK (originator_id >= 0),
    PRIMARY KEY (entity_id, entity_kind, originator_id)
);

CREATE TABLE key_package_history (
    id SERIAL PRIMARY KEY NOT NULL,
    key_package_hash_ref BYTEA NOT NULL UNIQUE,
    created_at_ns BIGINT NOT NULL,
    delete_at_ns BIGINT,
    post_quantum_public_key BYTEA
);

CREATE TABLE local_commit_log (
    -- A locally assigned ID for the local log entry
    rowid SERIAL PRIMARY KEY NOT NULL,
    group_id BYTEA NOT NULL,
    -- The sequence ID of the commit being applied.
    -- For welcomes, this is the sequence ID of the commit that spawned the welcome.
    -- For group creation, this is 0.
    commit_sequence_id BIGINT NOT NULL,
    -- The encryption state of the group before the commit was applied
    -- https://www.rfc-editor.org/rfc/rfc9420.html#section-8-13
    last_epoch_authenticator BYTEA NOT NULL,
    -- 1 = Applied, all other values are failures matching the protobuf
    commit_result INTEGER NOT NULL,
    -- The state after the commit was applied, or the existing state otherwise
    applied_epoch_number BIGINT NOT NULL,
    applied_epoch_authenticator BYTEA NOT NULL,
    -- Items below this line are for debugging purposes
    error_message TEXT,
    sender_inbox_id TEXT,
    sender_installation_id BYTEA,
    commit_type TEXT
);

-- SQLite backed `rowid` with its implicit pseudo-column; Postgres needs a real
-- SERIAL column so diesel's `remote_commit_log (rowid)` primary key resolves.
CREATE TABLE remote_commit_log (
    rowid SERIAL PRIMARY KEY NOT NULL,
    -- The sequence ID of the log entry on the server
    log_sequence_id BIGINT NOT NULL,
    group_id BYTEA NOT NULL,
    -- The sequence ID of the commit being referenced
    commit_sequence_id BIGINT NOT NULL,
    -- 1 = Applied, all other values are failures matching the protobuf
    commit_result INTEGER NOT NULL,
    -- The state after the commit was applied, or the existing state otherwise
    applied_epoch_number BIGINT NOT NULL,
    applied_epoch_authenticator BYTEA NOT NULL
);

CREATE TABLE readd_status (
    group_id BYTEA NOT NULL,
    installation_id BYTEA NOT NULL,
    requested_at_sequence_id BIGINT,
    responded_at_sequence_id BIGINT,
    PRIMARY KEY (group_id, installation_id)
);

-- Key column order follows diesel's `pending_remove (group_id, inbox_id)` tuple;
-- SQLite declared the same pair in the opposite order, which is equivalent for
-- uniqueness and differs only in the backing index's column order.
CREATE TABLE pending_remove (
    group_id BYTEA NOT NULL,
    inbox_id TEXT NOT NULL,
    message_id BYTEA NOT NULL,
    PRIMARY KEY (group_id, inbox_id)
);

CREATE TABLE processed_device_sync_messages (
    message_id BYTEA PRIMARY KEY NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0,
    state INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE tasks (
    id SERIAL PRIMARY KEY NOT NULL,
    -- ID of the message that originated the task
    originating_message_sequence_id BIGINT NOT NULL CHECK (originating_message_sequence_id >= 0),
    originating_message_originator_id INTEGER NOT NULL CHECK (originating_message_originator_id >= 0),
    created_at_ns BIGINT NOT NULL,
    expires_at_ns BIGINT NOT NULL,
    -- Number of attempts to execute the task; set to 1 initially
    attempts INTEGER NOT NULL,
    max_attempts INTEGER NOT NULL,
    last_attempted_at_ns BIGINT NOT NULL,
    -- Scaling factor between attempts
    backoff_scaling_factor REAL NOT NULL,
    max_backoff_duration_ns BIGINT NOT NULL,
    initial_backoff_duration_ns BIGINT NOT NULL,
    next_attempt_at_ns BIGINT NOT NULL,
    -- Hash of the data to execute the task, required for deduplication
    data_hash BYTEA NOT NULL UNIQUE,
    -- A serialized xmtp.mls.database.Task protobuf message
    data BYTEA NOT NULL
);

-- Singleton: the row is always written with an explicit id of 0, so this keeps
-- SQLite's literal `DEFAULT 0 CHECK (id = 0)` rather than becoming a SERIAL whose
-- nextval would immediately violate the CHECK.
CREATE TABLE user_preferences (
    id INTEGER PRIMARY KEY NOT NULL DEFAULT 0 CHECK (id = 0),
    -- HMAC root key
    hmac_key BYTEA,
    hmac_key_cycled_at_ns BIGINT,
    dm_group_updates_migrated BOOLEAN NOT NULL DEFAULT FALSE
);

-- Singleton, seeded below with an explicit id of 1; same reasoning as
-- user_preferences for keeping a plain INTEGER key instead of a SERIAL.
CREATE TABLE d14n_migration_cutover (
    id INTEGER PRIMARY KEY NOT NULL DEFAULT 1 CHECK (id = 1),
    cutover_ns BIGINT NOT NULL DEFAULT 9223372036854775807,
    last_checked_ns BIGINT NOT NULL DEFAULT 0,
    has_migrated BOOLEAN NOT NULL DEFAULT FALSE
);

INSERT INTO d14n_migration_cutover (id, cutover_ns, last_checked_ns, has_migrated)
VALUES (1, 9223372036854775807, 0, FALSE);

-- Allow for efficient sorting of groups
CREATE INDEX groups_created_at_idx ON "groups" (created_at_ns);

-- Filter by membership_state and then created_at_ns
CREATE INDEX groups_membership_state_created_at_idx ON "groups" (membership_state, created_at_ns);

-- Supports the EXISTS-based DM deduplication query:
-- WHERE COALESCE(dm_id, id) = ? AND last_message_ns > ?
-- SQLite can COALESCE a TEXT column with a BLOB one; Postgres requires a single
-- type, so the group id is hex-encoded into the text domain. DM ids are always
-- prefixed with 'dm:' and can therefore never collide with a hex-encoded id.
CREATE INDEX idx_groups_dm_coalesce_last_message
    ON "groups" ((COALESCE(dm_id, ENCODE(id, 'hex'))), last_message_ns DESC);

CREATE INDEX idx_groups_dm_id ON "groups" (dm_id) WHERE dm_id IS NOT NULL;

CREATE INDEX group_messages_sent_at_sort ON group_messages (group_id, sent_at_ns);
CREATE INDEX group_messages_reference_id ON group_messages (reference_id);
CREATE INDEX group_messages_inserted_at_sort ON group_messages (group_id, inserted_at_ns);
CREATE INDEX idx_group_messages_expire_at_ns ON group_messages (expire_at_ns) WHERE expire_at_ns IS NOT NULL;

CREATE INDEX group_intents_group_id_state ON group_intents (group_id, state);

CREATE INDEX idx_identity_updates_inbox_id_sequence_id_asc ON identity_updates (inbox_id, sequence_id ASC);

CREATE INDEX idx_icebox_group_id ON icebox (group_id);
CREATE INDEX idx_icebox_deps_lookup ON icebox_dependencies (dependency_originator_id, dependency_sequence_id);

CREATE INDEX idx_message_deletions_deleted_message_id ON message_deletions (deleted_message_id);
CREATE INDEX idx_message_deletions_group_id ON message_deletions (group_id);

CREATE INDEX idx_local_commit_log_group_id ON local_commit_log (group_id);
CREATE INDEX idx_remote_commit_log_group_id ON remote_commit_log (group_id);

-- Keeps groups.last_message_ns monotonically forward as messages arrive.
-- SQLite expresses this as a multi-statement trigger body; Postgres needs the
-- body in a function. AFTER-INSERT triggers ignore the return value, so NULL.
CREATE FUNCTION msg_inserted_fn() RETURNS TRIGGER AS $$
BEGIN
    UPDATE "groups"
    SET last_message_ns = NEW.sent_at_ns
    WHERE id = NEW.group_id
        AND (
            last_message_ns IS NULL
            OR NEW.sent_at_ns > last_message_ns
        );
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER msg_inserted
AFTER INSERT ON group_messages
FOR EACH ROW
EXECUTE FUNCTION msg_inserted_fn();

-- Filters for readable content types only, or content types with a text fallback.
--
-- Content type numeric values come from group_message.rs:
--   Unknown = 0, Text = 1, GroupMembershipChange = 2, GroupUpdated = 3,
--   Reaction = 4, ReadReceipt = 5, Reply = 6, Attachment = 7,
--   RemoteAttachment = 8, TransactionReference = 9, MultiRemoteAttachment = 10
CREATE VIEW conversation_list AS
WITH ranked_messages AS (
    SELECT
        gm.group_id,
        gm.id AS message_id,
        gm.decrypted_message_bytes,
        gm.sent_at_ns,
        gm.kind AS message_kind,
        gm.sender_installation_id,
        gm.sender_inbox_id,
        gm.delivery_status,
        gm.content_type,
        gm.version_major,
        gm.version_minor,
        gm.authority_id,
        gm.sequence_id,
        gm.originator_id,
        ROW_NUMBER() OVER (PARTITION BY gm.group_id ORDER BY gm.sent_at_ns DESC) AS row_num
    FROM
        group_messages gm
    WHERE
        gm.kind = 1
        AND gm.content_type IN (0, 1, 4, 6, 7, 8, 9, 10)
)
SELECT
    g.id AS id,
    g.created_at_ns,
    g.membership_state,
    g.installations_last_checked,
    g.added_by_inbox_id,
    g.sequence_id AS welcome_sequence_id,
    g.dm_id,
    g.rotated_at_ns,
    g.conversation_type,
    g.is_commit_log_forked,
    rm.message_id,
    rm.decrypted_message_bytes,
    rm.sent_at_ns,
    rm.message_kind,
    rm.sender_installation_id,
    rm.sender_inbox_id,
    rm.delivery_status,
    rm.content_type,
    rm.version_major,
    rm.version_minor,
    rm.authority_id,
    rm.sequence_id,
    rm.originator_id
FROM
    "groups" g
    LEFT JOIN ranked_messages rm
    ON g.id = rm.group_id AND rm.row_num = 1
ORDER BY COALESCE(rm.sent_at_ns, g.created_at_ns) DESC;
