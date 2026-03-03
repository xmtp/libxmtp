#!/bin/bash

set -euo pipefail

: "${XDBG_LOOP_PAUSE:=300}" # default interval between restarts

MESSAGE_DB=xdbg-message-db
GROUP_DB=xdbg-group-db
IDENTITY_DB=xdbg-identity-db

MSG_LOG=xdbg-scheduled-messages.out
GRP_LOG=xdbg-scheduled-groups.out
ID_LOG=xdbg-scheduled-identities.out

function log {
    echo "[$(date '+%F %T')] $*"
}

# Determine backend from WORKSPACE
WORKSPACE="${WORKSPACE:-}"
case "${WORKSPACE}" in
    testnet) BACKEND="production" ;;
    testnet-dev) BACKEND="dev" ;;
    testnet-staging) BACKEND="staging" ;;
    ""|*) BACKEND="local" ;;
esac
log "WORKSPACE='${WORKSPACE:-<unset>}' -> backend='${BACKEND}'"

function clear_db() {
    local db_root=$1
    mkdir -p "$db_root"
    log "Clearing DB at $db_root"
    XDBG_DB_ROOT="$db_root" xdbg -d -b "${BACKEND}" --clear || log "Clear failed"
    sleep 10
}

function generate_identities() {
    local db_root=$1
    mkdir -p "$db_root"
    log "Generating identities at $db_root"
    # Fast pass (no delay), then a conservative retry with 5s pause if needed
    XDBG_LOOP_PAUSE=0 XDBG_DB_ROOT="$db_root" xdbg -d -b "${BACKEND}" generate --entity identity --amount 10 --concurrency 1 \
        || { log "Identity generation failed, clearing and retrying"; clear_db "$db_root"; XDBG_LOOP_PAUSE=5 XDBG_DB_ROOT="$db_root" xdbg -d -b "${BACKEND}" generate --entity identity --amount 10 --concurrency 1; }
}

function generate_groups_with_retry() {
    local db_root=$1
    local attempts=0
    local max_attempts=5

    while [ $attempts -lt $max_attempts ]; do
        log "Sleeping 60 seconds before attempting group generation..."
        sleep 60
        log "Attempting group generation (attempt $((attempts + 1)))"
        if XDBG_DB_ROOT="$db_root" xdbg -d -b "${BACKEND}" generate --entity group --invite 1 --amount 1 --concurrency 1; then
            return 0
        else
            log "Group generation failed. Clearing DB and retrying..."
            clear_db "$db_root"
            generate_identities "$db_root"
            sleep 5
            attempts=$((attempts + 1))
        fi
    done

    log "Group generation failed after $max_attempts attempts. Exiting."
    exit 1
}

function setup_data() {
    local db_root=$1
    local for_entity=$2

    log "Setting up data for $for_entity (db=$db_root)"
    generate_identities "$db_root"

    if [ "$for_entity" = "message" ]; then
        log "Sleeping 60 seconds before group generation for messages"
        sleep 60
        generate_groups_with_retry "$db_root"
    fi
}

# --- Singleton long-runners (exactly one per entity) ------------------------

# Generic singleton runner (message/group)
function run_long_test_singleton() {
    local db_root=$1
    local entity=$2
    local log_file=$3

    mkdir -p "$db_root"

    # Lock ensures only one instance per entity on the host.
    # Stored beside the DB root (not /tmp) to keep all ephemeral state in one place.
    local lockfile="${db_root}/.xdbg-${entity}.lock"
    exec 9>"$lockfile"
    if ! flock -n 9; then
        log "Another ${entity} runner is already active (lock: $lockfile). Not starting a duplicate."
        return 0
    fi

    # Clean up lock on exit
    trap 'log "Stopping ${entity} runner"; rm -f "$lockfile" || true' EXIT

    log "Starting singleton ${entity} runner with DB at $db_root (lock: $lockfile)"

    while true; do
        # Inner loop: call xdbg with --amount 1 repeatedly, exit on first failure
        while true; do
            ## MUST pass --invite now so the operation hits the network. Message send just ignores --invite so no problem
            if ! XDBG_LOOP_PAUSE=0 XDBG_DB_ROOT="$db_root" xdbg -d -b "${BACKEND}" generate --entity "$entity" --amount 1 --concurrency 1 --invite 1 >>"$log_file" 2>&1; then
                log "${entity} xdbg failed; breaking loop to trigger repairs."
                break
            fi
            sleep "${XDBG_LOOP_PAUSE}"
        done

        log "${entity} generator encountered a failure. Resetting DB and repairing prerequisites..."
        clear_db "$db_root"
        setup_data "$db_root" "$entity"

        log "Restarting ${entity} generator in ${XDBG_LOOP_PAUSE}s..."
        sleep "${XDBG_LOOP_PAUSE}"
    done
}

# Identity-only singleton with special fast-repair path
function run_identity_long_test_singleton() {
    local db_root=$1
    local log_file=$2

    mkdir -p "$db_root"

    # Stored beside the DB root (not /tmp) to keep all ephemeral state in one place.
    local lockfile="${db_root}/.xdbg-identity.lock"
    exec 10>"$lockfile"
    if ! flock -n 10; then
        log "Another identity runner is already active (lock: $lockfile). Not starting a duplicate."
        exec 10>&-
        return 0
    fi

    trap 'log "Stopping identity runner"; rm -f "$lockfile" || true' EXIT

    log "Starting singleton identity runner with DB at $db_root (lock: $lockfile)"

    while true; do
        # Inner loop: one-shot identity generation; exit loop on first failure
        while true; do
            if ! XDBG_LOOP_PAUSE=0 XDBG_DB_ROOT="$db_root" xdbg -d -b "${BACKEND}" generate --entity identity --amount 1 --concurrency 1 >>"$log_file" 2>&1; then
                log "identity xdbg failed; breaking inner loop to trigger repairs."
                break
            fi
            sleep "${XDBG_LOOP_PAUSE}"
        done

        log "Identity generator failed. Clearing DB and performing quick repairs..."
        clear_db "$db_root"
        # repairs: override pause to 5s for quick identity seeding only for this repair step
        XDBG_LOOP_PAUSE=5 XDBG_DB_ROOT="$db_root" xdbg -d -b "${BACKEND}" generate --entity identity --amount 10 --concurrency 1 || true

        log "Restarting identity generator in ${XDBG_LOOP_PAUSE}s..."
        sleep "${XDBG_LOOP_PAUSE}"
    done
}

# --- Bootstrap required data for message/group, none for identity -----------

setup_data "$GROUP_DB" group
setup_data "$MESSAGE_DB" message

echo "Starting tests...."
run_long_test_singleton "$GROUP_DB" group "$GRP_LOG" &
run_long_test_singleton "$MESSAGE_DB" message "$MSG_LOG" &
run_identity_long_test_singleton "$IDENTITY_DB" "$ID_LOG" &
wait
