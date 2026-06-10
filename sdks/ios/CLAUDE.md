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

# Or use just commands (they use Nix automatically)
just ios build
```

### First-time setup: Xcode in the Nix store

On macOS, iOS builds need Xcode imported into /nix/store (Apple's license
forbids redistributing it via binary caches). `just ios build` handles this
automatically on first run (~5 min) if `/Applications/Xcode_26.3.app`
exists. Install it with:

```bash
brew install xcodes
xcodes install 26.3
```

The version must match `xcodeVer` in `nix/ios-packages.nix`. Override with
`XCODE_VERSION` / `XCODE_APP` env vars if your install lives elsewhere.
CI imports Xcode via the xmtp-cache-apple action instead.

## Development Commands

All commands run through justfile recipes with Nix:

```bash
just ios build          # Build libxmtp xcframework
just ios check          # Build bindings + Swift package
just ios test           # Run Swift tests
just ios lint           # Run SwiftLint + SwiftFormat check
just ios format         # Format code with SwiftFormat
```

## Building the xcframework

The Swift package depends on `LibXMTPSwiftFFI.xcframework` which is built from the Rust code in `bindings/mobile/`. Run `./sdks/ios/dev/bindings` to rebuild it when Rust code changes.

The xcframework is output to `.build/LibXMTPSwiftFFI.xcframework`.

## Testing

Tests require a running XMTP backend. For CI, tests use ephemeral Fly.io infrastructure. For local testing:

```bash
# Start local backend (from repo root)
just backend up

# Run tests (from repo root)
just ios test
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
