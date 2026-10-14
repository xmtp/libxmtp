# shellcheck shell=bash
# Prove each service with its own client, through the exported variables.
set -euo pipefail

for name in DATABASE_URL XMTP_REPLICA_URL XMTP_BACKEND_URL XMTP_BACKEND_TOXIC_URL \
  XMTP_TOXIPROXY_API ANVIL_URL XMTP_S3_URL XMTP_S3_BASE_URL XMTP_BACKEND_METRICS_PORT; do
  [ -n "${!name:-}" ] || { echo "missing $name" >&2; exit 1; }
  echo "$name=${!name}"
done
grep -q "^export DATABASE_URL=" "$XMTP_SERVICES_STATE/env"

[ "$(psql "$DATABASE_URL" -Atc 'SELECT pg_is_in_recovery()')" = f ]
[ "$(psql "$XMTP_REPLICA_URL" -Atc 'SELECT pg_is_in_recovery()')" = t ]
psql "$DATABASE_URL" -Atc 'CREATE TABLE services_check (id int); INSERT INTO services_check VALUES (7)'
lsn=$(psql "$DATABASE_URL" -Atc 'SELECT pg_current_wal_lsn()')
for _ in $(seq 100); do
  [ "$(psql "$XMTP_REPLICA_URL" -Atc "SELECT pg_last_wal_replay_lsn() >= '$lsn'")" = t ] && break
  sleep 0.1
done
[ "$(psql "$XMTP_REPLICA_URL" -Atc 'SELECT id FROM services_check')" = 7 ]
echo "postgres: primary writes, replica streams"

export AWS_ACCESS_KEY_ID=xmtps3 AWS_SECRET_ACCESS_KEY=xmtps3secret AWS_DEFAULT_REGION=us-east-1
export AWS_EC2_METADATA_DISABLED=true AWS_CONFIG_FILE=/dev/null AWS_SHARED_CREDENTIALS_FILE=/dev/null
s3() { aws --endpoint-url "$XMTP_S3_URL" s3api "$@"; }
s3 get-bucket-cors --bucket attachments | jq -e '.CORSRules[0].AllowedOrigins == ["*"]' >/dev/null
s3 get-bucket-policy --bucket attachments | jq -e '.Policy | fromjson | .Statement[0].Action == "s3:GetObject"' >/dev/null
printf services_check >"$XMTP_SERVICES_STATE/object"
s3 put-object --bucket attachments --key services_check --body "$XMTP_SERVICES_STATE/object" >/dev/null
[ "$(curl -sf "$XMTP_S3_BASE_URL/services_check")" = services_check ]
# VersityGW sends a body of 64 KiB or more with sendfile(2), which a Darwin
# sandbox denies unless the derivation uses the services' sandbox profile.
head -c 1048576 /dev/zero >"$XMTP_SERVICES_STATE/large"
s3 put-object --bucket attachments --key services_check_large \
  --body "$XMTP_SERVICES_STATE/large" >/dev/null
[ "$(curl -sf "$XMTP_S3_BASE_URL/services_check_large" | wc -c)" -eq 1048576 ]
echo "s3: bucket, policy, CORS, public read, large public read"

curl -sf -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"eth_chainId","params":[]}' "$ANVIL_URL" |
  jq -e '.result == "0x7a69"' >/dev/null
echo "anvil: chain 31337"

grpc-health-probe -addr="${XMTP_BACKEND_URL#http://}"
grpc-health-probe -addr="${XMTP_BACKEND_TOXIC_URL#http://}"
# Not `curl | grep -q`: under pipefail, grep exiting first fails curl with SIGPIPE.
[ -n "$(curl -sf "http://127.0.0.1:$XMTP_BACKEND_METRICS_PORT/metrics")" ]
curl -sf "$XMTP_TOXIPROXY_API/proxies" | jq -e '.backend.enabled' >/dev/null
echo "backend: gRPC health direct and through toxiproxy, metrics"
