#!/bin/bash

# Build script for XMTP CLI tools (stream-monitor and message-sender)
# Compiles both binaries in release mode with optimizations

set -e  # Exit on any error

# Change to the libxmtp root directory (two levels up from examples/cli)
cd "$(dirname "$0")/../.."

# Build both binaries in release mode
echo "Building stream-monitor and message-sender in release mode..."
cargo build --release --bin stream-monitor --bin message-sender

# Check if builds were successful
if [ -f "target/release/stream-monitor" ] && [ -f "target/release/message-sender" ]; then
    echo ""
    echo "✅ Build successful!"
    echo ""
    echo "Release binaries created:"
    echo "  📄 stream-monitor: $(realpath target/release/stream-monitor)"
    echo "  📄 message-sender: $(realpath target/release/message-sender)"
else
    echo ""
    echo "❌ Build failed - binaries not found"
    exit 1
fi