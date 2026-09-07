#!/usr/bin/env bash
set -euo pipefail

if [[ ! -s "$PGDATA/PG_VERSION" ]]; then
  pg_basebackup --host=primary --username=replicator --pgdata="$PGDATA" \
    --wal-method=stream --write-recovery-conf
fi
exec postgres -c hot_standby=on
