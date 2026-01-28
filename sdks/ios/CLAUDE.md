# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Development Commands

### Building

```bash
# Build the Swift package
swift build

# Build for specific platform
swift build --platform ios
```

### Testing

```bash
# Run tests with retry logic (recommended due to external dependencies)
./script/run_tests.sh

# Run tests directly
swift test

# Run tests with verbose output
swift test -v | grep -E "Test Case|XCTAssert|failures"
```

### Linting

```bash
# Run SwiftLint (must be installed separately)
./dev/lint

# Validate CocoaPods spec
pod lib lint --allow-warnings
```

### Local Development Environment

```bash
# Start local test server (requires Docker)
./script/local

# Alternative Docker setup
./dev/up
```

## High-Level Architecture

### Core Components

**Client** (`Sources/XMTPiOS/Client.swift`)

- Entry point for XMTP SDK functionality
- Manages API connections and authentication
- Handles inbox creation and management
- Supports V3 protocol (MLS-based) - V2 is deprecated as of May 1, 2025

**Conversation System**

- `Conversation.swift`: Unified interface for both Groups and DMs
- `Group.swift`: MLS-based group messaging implementation
- `Dm.swift`: Direct messaging implementation
- Both support disappearing messages and various content types

**Codec System** (`Sources/XMTPiOS/Codecs/`)

- Extensible content type system for messages
- Built-in codecs: Text, Attachment, RemoteAttachment, Reaction, Reply, ReadReceipt, GroupUpdated, TransactionReference
- Protocol-based design allowing custom content types

**LibXMTP Integration**

- Swift bindings to Rust-based `libxmtp-swift` (v4.3.6)
- Handles cryptographic operations and MLS protocol
- Database operations with encryption support

### Key Architectural Patterns

1. **Protocol-Oriented Design**: Heavy use of Swift protocols for extensibility (ContentCodec, SigningKey)

2. **Actor-Based Concurrency**: Uses Swift actors for thread-safe operations (ApiClientCache)

3. **Protobuf Messaging**: All wire formats defined in `.proto` files, generated Swift code in `Proto/` directory

4. **Test Helpers**: Separate `XMTPTestHelpers` module for testing utilities

### Environment Configuration

- **Development**: `.dev` environment for testing
- **Production**: `.production` for live usage
- **Local**: `.local` for Docker-based local development
- History sync URLs configured per environment

### Dependencies

- `CSecp256k1`: Cryptographic operations
- `Connect-Swift`: gRPC connectivity
- `CryptoSwift`: Additional cryptographic utilities
- `LibXMTP`: Core XMTP protocol implementation in Rust

### Important Notes

- The SDK requires iOS 14+ or macOS 11+
- Database encryption key must be provided in ClientOptions
- Device sync and history sync are configurable features
- All conversations use MLS (Message Layer Security) protocol
