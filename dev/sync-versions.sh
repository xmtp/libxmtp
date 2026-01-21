#!/bin/bash
set -eou pipefail

# Check if jq is installed
if ! command -v jq &> /dev/null; then
    echo "Error: jq is required but not installed. Please install jq first."
    exit 1
fi

# Check if cargo is installed
if ! command -v cargo &> /dev/null; then
    echo "Error: cargo is required but not installed. Please install Rust and cargo first."
    exit 1
fi

# Extract version from Cargo metadata
echo "Extracting crate versions from Cargo metadata..."
CARGO_METADATA=$(cargo metadata --format-version=1)

# Extract bindings_node version
if [ -d "bindings/node" ]; then
    BINDINGS_NODE_VERSION=$(echo "$CARGO_METADATA" | jq -r '.packages[] | select(.name == "bindings_node") | .version')

    if [ -z "$BINDINGS_NODE_VERSION" ]; then
        echo "Error: Failed to extract version for bindings_node crate."
        exit 1
    fi

    echo "Found bindings_node version: $BINDINGS_NODE_VERSION"

    # Update bindings/node/package.json
    if [ -f "bindings/node/package.json" ]; then
        echo "Updating bindings/node/package.json version to $BINDINGS_NODE_VERSION"
        jq ".version = \"$BINDINGS_NODE_VERSION\"" bindings/node/package.json > bindings/node/package.json.tmp
        mv bindings/node/package.json.tmp bindings/node/package.json
        echo "✅ Updated bindings/node/package.json"
    fi
fi

# Extract bindings_wasm version
if [ -d "bindings/wasm" ]; then
    BINDINGS_WASM_VERSION=$(echo "$CARGO_METADATA" | jq -r '.packages[] | select(.name == "bindings_wasm") | .version')

    if [ -z "$BINDINGS_WASM_VERSION" ]; then
        echo "Error: Failed to extract version for bindings_wasm crate."
        exit 1
    fi

    echo "Found bindings_wasm version: $BINDINGS_WASM_VERSION"

    # Update bindings/wasm/package.json
    if [ -f "bindings/wasm/package.json" ]; then
        echo "Updating bindings/wasm/package.json version to $BINDINGS_WASM_VERSION"
        jq ".version = \"$BINDINGS_WASM_VERSION\"" bindings/wasm/package.json > bindings/wasm/package.json.tmp
        mv bindings/wasm/package.json.tmp bindings/wasm/package.json
        echo "✅ Updated bindings/wasm/package.json"
    fi
fi

echo "✨ Version sync complete!"
