#!/usr/bin/env bash
set -euo pipefail

if [[ ! -s "$PGDATA/PG_VERSION" ]]; then
  pg_basebackup --host=db --username=replicator --pgdata="$PGDATA" \
    --wal-method=stream --write-recovery-conf
fi
exec postgres -c hot_standby=on
