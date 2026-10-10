#!/usr/bin/env bash
# Run one command with disposable backend test services: a PostgreSQL primary,
# a streaming PostgreSQL replica, and VersityGW for S3. Each service listens on
# a free loopback port. The command receives the same variables that the
# Docker stack gives backend tests. All services stop when the command exits.
#
# backend-tests.nix runs its check phase through this script. It supplies
# PostgreSQL, VersityGW, the AWS CLI, and Python on PATH, and the bucket policy
# and CORS files of the Docker stack in S3_POLICY and S3_CORS.
set -euo pipefail

[[ $# -gt 0 ]] || {
  echo 'Usage: backend-tests.sh COMMAND [ARG ...]' >&2
  exit 2
}
: "${S3_POLICY:?}" "${S3_CORS:?}"
state="$(mktemp -d "${TMPDIR:-/tmp}/backend-test-services.XXXXXX")"
# The Docker stack keeps PostgreSQL data in tmpfs. Use it where Linux has one,
# so that the replica replays the database churn of the tests as fast.
data="$state"
if [[ "$(uname -s)" == Linux && -d /dev/shm && -w /dev/shm ]]; then
  data="$(mktemp -d /dev/shm/backend-test-services.XXXXXX)"
fi
startup_seconds=120
services=()
started=false

cleanup() {
  result=$?
  trap - EXIT INT TERM
  for cluster in "$data/replica" "$data/primary"; do
    if [[ -f "$cluster/postmaster.pid" ]]; then
      pg_ctl --pgdata="$cluster" --mode=immediate --wait stop >/dev/null 2>&1 || true
    fi
  done
  for pid in "${services[@]}"; do
    kill -TERM "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
  done
  # Show service logs for a startup failure. A failed command keeps its own
  # output at the end of the log.
  if ((result != 0)) && ! "$started"; then
    for log in "$state"/*.log; do
      [[ -f "$log" ]] || continue
      echo "--- last lines of $(basename "$log")" >&2
      tail -n 40 "$log" >&2
    done
  fi
  rm -rf "$data" "$state"
  exit "$result"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

# Wait for a readiness command. Fail when a service exits or time runs out.
wait_for() {
  local name=$1
  shift
  until "$@" >/dev/null 2>&1; do
    for pid in "${services[@]}"; do
      kill -0 "$pid" 2>/dev/null || {
        echo "A test service exited while $name started" >&2
        exit 1
      }
    done
    if ((SECONDS > startup_seconds)); then
      echo "$name did not start in $startup_seconds seconds" >&2
      exit 1
    fi
    sleep 0.1
  done
}

# Free loopback ports. A sandbox has its own loopback interface; a build
# without one shares the host's, so fixed ports could be in use.
read -r primary_port replica_port s3_port < <(
  python3 -c '
import socket
sockets = [socket.socket() for _ in range(3)]
for s in sockets:
    s.bind(("127.0.0.1", 0))
print(*(s.getsockname()[1] for s in sockets))
'
)
[[ -n "${s3_port:-}" ]] || {
  echo 'Could not reserve loopback ports' >&2
  exit 1
}

# Disposable data needs no durable writes.
postgres_config() {
  printf '%s\n' \
    "listen_addresses = '127.0.0.1'" \
    "port = $1" \
    "unix_socket_directories = ''" \
    "fsync = off"
}

initdb --pgdata="$data/primary" --username=xmtp --encoding=UTF8 --locale=C \
  --auth=trust >"$state/primary.log" 2>&1
{
  postgres_config "$primary_port"
  # The replica reads retained WAL after replay pauses in replica tests.
  echo "wal_keep_size = '256MB'"
} >>"$data/primary/postgresql.conf"
postgres -D "$data/primary" >>"$state/primary.log" 2>&1 </dev/null &
services+=("$!")
wait_for PostgreSQL pg_isready --host=127.0.0.1 --port="$primary_port" --username=xmtp
createdb --host=127.0.0.1 --port="$primary_port" --username=xmtp xmtp_backend

pg_basebackup --host=127.0.0.1 --port="$primary_port" --username=xmtp \
  --pgdata="$data/replica" --wal-method=stream --write-recovery-conf \
  --checkpoint=fast >"$state/replica.log" 2>&1
{
  postgres_config "$replica_port"
  echo 'hot_standby = on'
} >>"$data/replica/postgresql.conf"
postgres -D "$data/replica" >>"$state/replica.log" 2>&1 </dev/null &
services+=("$!")
wait_for 'PostgreSQL replica' pg_isready --host=127.0.0.1 --port="$replica_port" --username=xmtp

export XMTP_S3_URL="http://127.0.0.1:$s3_port"
mkdir -p "$state/objects"
env -i PATH="$PATH" HOME="$state" ROOT_ACCESS_KEY=xmtps3 ROOT_SECRET_KEY=xmtps3secret \
  versitygw --port "127.0.0.1:$s3_port" --cors-allow-origin '*' posix "$state/objects" \
  >"$state/s3.log" 2>&1 </dev/null &
services+=("$!")

# Isolate the AWS CLI from caller profiles, credentials, proxies and endpoints.
aws_local() {
  env -i PATH="$PATH" HOME="$state" \
    AWS_CONFIG_FILE="$state/aws-config" AWS_SHARED_CREDENTIALS_FILE="$state/aws-credentials" \
    AWS_ACCESS_KEY_ID=xmtps3 AWS_SECRET_ACCESS_KEY=xmtps3secret \
    AWS_EC2_METADATA_DISABLED=true AWS_PAGER= AWS_MAX_ATTEMPTS=1 \
    aws --endpoint-url "$XMTP_S3_URL" --region us-east-1 \
    --cli-connect-timeout 2 --cli-read-timeout 2 "$@"
}
wait_for VersityGW aws_local s3api list-buckets
{
  aws_local s3api create-bucket --bucket attachments
  aws_local s3api put-bucket-policy --bucket attachments --policy "file://$S3_POLICY"
  aws_local s3api put-bucket-cors --bucket attachments --cors-configuration "file://$S3_CORS"
} >>"$state/s3.log" 2>&1

export DATABASE_URL="postgres://xmtp:xmtp@127.0.0.1:$primary_port/xmtp_backend"
export XMTP_BACKEND_REPLICA_PORT="$replica_port"
export XMTP_S3_BASE_URL="$XMTP_S3_URL/attachments"
unset OTEL_EXPORTER_OTLP_ENDPOINT
echo "Backend test services ready after $SECONDS seconds"
started=true
"$@"
