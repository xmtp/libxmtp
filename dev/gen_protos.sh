#!/bin/bash

BRANCH=${PROTO_BRANCH:-main}
REV=$(git ls-remote https://github.com/xmtp/proto $BRANCH | awk '{print $1}')
WORKSPACE_MANIFEST="$(cargo locate-project --workspace --message-format=plain)"
WORKSPACE_PATH="$(dirname $WORKSPACE_MANIFEST)"

export GEN_PROTOS=1
echo $REV > $WORKSPACE_PATH/xmtp_proto/proto_version
cargo build -p xmtp_proto
