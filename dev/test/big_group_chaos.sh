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
TARGET_DIR="$(cargo metadata --format-version 1 --no-deps | jq -r '.target_directory')"
CMD="${TARGET_DIR}/release/xdbg -b $NETWORK"

cargo build --release --bin xdbg
echo "writing groups to $EXPORT"
"${TARGET_DIR}"/release/xdbg --clear
$CMD --clear
$CMD generate --entity identity --amount 250
$CMD generate --entity group --amount 1 --invite 10
$CMD export --entity group --out "$EXPORT"
GROUP_ID=$(jq -r '.[0].id' "$EXPORT")
echo "group has id $GROUP_ID"
$CMD modify --inbox-id "$INBOX_ID" add-external "$GROUP_ID"
# generate 2 message every 1000 milliseconds, and add a new member (up to 200 members) + change the grp description
$CMD generate --entity message --amount 2 --interval 1000 --loop --add-and-change-description --add-up-to 200
