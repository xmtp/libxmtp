#!/bin/bash

BRANCH=${PROTO_BRANCH:-main}

pushd xmtp_proto > /dev/null
if ! cargo install --list | grep "protoc-gen-prost-crate" > /dev/null; then
    # installing from git b/c https://github.com/neoeinstein/protoc-gen-prost/pull/119
    if ! cargo install --git protoc-gen-prost-crate; then
        echo "Failed to install protoc-gen-prost-crate"
        exit 1
    fi
fi

echo "Generating protos from branch ${BRANCH}..."

if ! buf generate "https://github.com/xmtp/proto.git#branch=${BRANCH},subdir=proto"; then
    echo "Failed to generate protobuf definitions"
    exit 1
fi
popd > /dev/null
