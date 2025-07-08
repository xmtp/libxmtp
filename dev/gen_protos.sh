#!/bin/bash

set -euo pipefail

BRANCH=${PROTO_BRANCH:-main}
PROTO_DIR="xmtp_proto/proto"
THIRD_PARTY_DIR="xmtp_proto/third_party"
GEN_DIR="xmtp_proto/src/gen"
MOD_RS="$GEN_DIR/mod.rs"

echo "Fetching protos from XMTP proto repo branch: ${BRANCH}..."

rm -rf "${PROTO_DIR}"
mkdir -p "${PROTO_DIR}"

# Fetch and extract the proto files from the XMTP proto repo
curl -L "https://github.com/xmtp/proto/archive/refs/heads/${BRANCH}.tar.gz" |
  tar -xz --strip-components=2 --directory="${PROTO_DIR}" "proto-${BRANCH}/proto"

echo "Protos exported to ${PROTO_DIR}"

# Clone googleapis for google/api/annotations.proto
echo "Cloning googleapis into third_party..."
rm -rf "${THIRD_PARTY_DIR}/googleapis"
git clone --depth 1 https://github.com/googleapis/googleapis.git "${THIRD_PARTY_DIR}/googleapis"

# Clone grpc-gateway for protoc-gen-openapiv2/options/annotations.proto
echo "Cloning grpc-gateway into third_party..."
rm -rf "${THIRD_PARTY_DIR}/grpc-gateway"
git clone --depth 1 https://github.com/grpc-ecosystem/grpc-gateway.git "${THIRD_PARTY_DIR}/grpc-gateway"

echo "Protos and dependencies ready."

echo "Running cargo build to generate code with tonic_build..."
cargo build

echo "Generating mod.rs for all generated files..."
rm -f "$MOD_RS"
mkdir -p "$GEN_DIR"

echo "// @generated" > "$MOD_RS"
echo "pub mod xmtp {" >> "$MOD_RS"

find "$GEN_DIR" -maxdepth 1 -name "xmtp.*.rs" | while read -r file; do
    rel=$(basename "$file" .rs)
    path_parts=(${rel//./ })
    indent="    "
    modline=""
    for (( i=0; i<${#path_parts[@]}-1; i++ )); do
        echo "${indent}pub mod ${path_parts[$i]} {" >> "$MOD_RS"
        indent+="    "
    done
    fname=$(basename "$file")
    echo "${indent}include!(\"$fname\");" >> "$MOD_RS"
    for (( i=0; i<${#path_parts[@]}-1; i++ )); do
        indent=${indent:0:-4}
        echo "${indent}}" >> "$MOD_RS"
    done
done

echo "}" >> "$MOD_RS"

echo "Done generating $MOD_RS"
