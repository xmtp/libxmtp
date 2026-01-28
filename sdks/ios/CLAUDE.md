# XMTP iOS SDK - Claude Assistant Context

This SDK provides Swift bindings for the XMTP messaging protocol, built on top of libxmtp.

## Project Structure

- `Sources/XMTPiOS/` - Main SDK source code
- `Sources/XMTPTestHelpers/` - Test utilities
- `Tests/XMTPTests/` - Test suite
- `dev/` - Development scripts
- `.build/` - Generated artifacts (gitignored)

## Development Setup

This SDK lives within the libxmtp monorepo. Use the Nix development environment:

```bash
# From repository root
nix develop .#ios

# Or run scripts directly (they auto-enter Nix shell)
./sdks/ios/dev/build
```

## Development Commands

All scripts auto-detect Nix shell and enter it if needed:

```bash
./sdks/ios/dev/build    # Build libxmtp xcframework + Swift package
./sdks/ios/dev/test     # Run Swift tests
./sdks/ios/dev/lint     # Run SwiftLint
./sdks/ios/dev/fmt      # Format code with SwiftFormat
./sdks/ios/dev/fmt --lint  # Check formatting without changes
```

## Building the xcframework

The Swift package depends on `LibXMTPSwiftFFI.xcframework` which is built from the Rust code in `bindings/mobile/`. Run `./sdks/ios/dev/build` to rebuild it when Rust code changes.

The xcframework is output to `.build/LibXMTPSwiftFFI.xcframework`.

## Testing

Tests require a running XMTP backend. For CI, tests use ephemeral Fly.io infrastructure. For local testing:

```bash
# Start local backend (from repo root)
./dev/up

# Run tests
./sdks/ios/dev/test
```

Environment variables for custom backend:
- `XMTP_NODE_ADDRESS` - Node gRPC URL
- `XMTP_HISTORY_SERVER_ADDRESS` - History server URL

## Code Style

- **Formatting**: SwiftFormat (nicklockwood) - config in `.swiftformat`
- **Linting**: SwiftLint - config in `.swiftlint.yml` and `Tests/.swiftlint.yml`

## Key Dependencies

- `LibXMTPSwiftFFI` - FFI bindings from libxmtp (local path)
- `Connect` - gRPC client
- `CryptoSwift` - Cryptographic utilities
