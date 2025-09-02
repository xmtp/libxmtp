#!/bin/bash
# USAGE: ./big_group.sh <INBOX_ID> [NETWORK]
#   INBOX_ID: Required - The inbox ID to add to the external group
#   NETWORK:  Optional - Network to use (default: "dev")
#
# Example: ./big_group.sh fbeb081944df5ef3f26de65f05ada28c781ac3086f0a7ccff2751ede994ebfc9
# Example: ./big_group.sh fbeb081944df5ef3f26de65f05ada28c781ac3086f0a7ccff2751ede994ebfc9 dev
#
# Keep in mind the inbox id must already exist on the network you're trying to add it to.
#
# This script generates test data by creating 150 identities, 1 group with all identities,
# adds the specified inbox to that group, and generates 3 test messages in a loop every second.
# Requires: jq, cargo, and the xdbg binary to be buildable.

set -eou pipefail

if ! jq --version &>/dev/null; then echo "must install jq"; fi

INBOX_ID=$1
NETWORK=${2-"dev"}
EXPORT=$(mktemp)
CMD="./target/release/xdbg -b $NETWORK"

cargo build --release --bin xdbg
echo "writing groups to $EXPORT"
./target/release/xdbg --clear
$CMD --clear
$CMD generate --entity identity --amount 1
$CMD generate --entity group --amount 100

$CMD export --entity group --out $EXPORT
for i in $(seq 0 99); do
  GROUP_ID=$(jq -r ".[$i].id" $EXPORT)
  echo "group $i has id $GROUP_ID"
  $CMD modify --inbox-id $INBOX_ID add-external $GROUP_ID
  $CMD generate --entity message --amount 2
done
