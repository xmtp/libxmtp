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

# Extract version from Cargo metadata using a more direct approach
echo "Extracting workspace version from Cargo metadata..."
CARGO_METADATA=$(cargo metadata --format-version=1 --no-deps)
CARGO_VERSION=$(echo "$CARGO_METADATA" | jq -r '.workspace_metadata.package.version // empty')

# If workspace_metadata.package.version is not available, try to get it from the root package
if [ -z "$CARGO_VERSION" ]; then
    # Get the root package ID
    ROOT_PACKAGE=$(echo "$CARGO_METADATA" | jq -r '.resolve.root // empty')
    
    if [ -n "$ROOT_PACKAGE" ]; then
        # Extract version from the root package
        CARGO_VERSION=$(echo "$CARGO_METADATA" | jq -r --arg pkg "$ROOT_PACKAGE" '.packages[] | select(.id == $pkg) | .version')
    fi
fi

# If still empty, try to extract from workspace.package.version in Cargo.toml directly
if [ -z "$CARGO_VERSION" ]; then
    echo "Falling back to parsing Cargo.toml directly..."
    CARGO_VERSION=$(grep -A 2 '\[workspace.package\]' Cargo.toml | grep 'version' | sed -E 's/version[[:space:]]*=[[:space:]]*"([^"]*)"/\1/')
fi

if [ -z "$CARGO_VERSION" ]; then
    echo "Error: Failed to extract version from Cargo metadata."
    exit 1
fi

echo "Found Cargo workspace version: $CARGO_VERSION"

# Update bindings_node/package.json
if [ -f "bindings_node/package.json" ]; then
    echo "Updating bindings_node/package.json version to $CARGO_VERSION"
    jq ".version = \"$CARGO_VERSION\"" bindings_node/package.json > bindings_node/package.json.tmp
    mv bindings_node/package.json.tmp bindings_node/package.json
    echo "✅ Updated bindings_node/package.json"
fi

# Update bindings_wasm/package.json
if [ -f "bindings_wasm/package.json" ]; then
    echo "Updating bindings_wasm/package.json version to $CARGO_VERSION"
    jq ".version = \"$CARGO_VERSION\"" bindings_wasm/package.json > bindings_wasm/package.json.tmp
    mv bindings_wasm/package.json.tmp bindings_wasm/package.json
    echo "✅ Updated bindings_wasm/package.json"
fi

echo "✨ Version sync complete: $CARGO_VERSION"