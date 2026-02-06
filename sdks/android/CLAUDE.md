# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is the XMTP Android SDK repository - a native Android SDK written in Kotlin that provides XMTP messaging functionality for Android applications. The project implements the XMTP protocol natively on Android and includes a comprehensive example application demonstrating the SDK's capabilities.

## Common Development Commands

### Build & Development
- `./gradlew build` - Build the entire project
- `./gradlew :library:build` - Build only the library module
- `./gradlew :example:build` - Build only the example app
- `./gradlew clean` - Clean build artifacts
- `./gradlew assembleDebug` - Build debug variant
- `./gradlew assembleRelease` - Build release variant

### Code Quality
- `./gradlew ktlintCheck` - Run ktlint code style checks
- `./gradlew ktlintFormat` - Auto-format code with ktlint
- `./gradlew lint` - Run Android lint checks
- `./gradlew test` - Run unit tests
- `./gradlew connectedAndroidTest` - Run instrumented tests (requires device/emulator)

### Documentation
- `./gradlew dokkaGfmPartial` - Generate documentation
- `./gradlew dokkaHtml` - Generate HTML documentation

### Publishing
- `./gradlew publishToMavenLocal` - Publish to local Maven repository
- `./gradlew publishToSonatype` - Publish to Sonatype (requires credentials)

## Architecture

### Core Structure
- **library/**: Main SDK source code
  - **src/main/java/org/xmtp/android/library/**: Core SDK implementation
    - **Client.kt**: Main XMTP client for managing connections
    - **Conversation.kt**: Conversation management
    - **Group.kt**: Group messaging functionality
    - **Dm.kt**: Direct messaging functionality
    - **codecs/**: Content type codecs (Text, Reply, ReadReceipt, GroupUpdated)
    - **push/**: Push notification support
- **example/**: Example Android application demonstrating SDK usage
  - **src/main/java/org/xmtp/android/example/**: Example app implementation
- **library/src/androidTest/**: Instrumented tests
- **library/src/test/**: Unit tests

### Key Components
- **Client**: Main XMTP client for authentication and conversation management
- **Conversation**: Base class for all conversation types
- **Group**: Group messaging with MLS (Messaging Layer Security)
- **Dm**: Direct messaging functionality
- **CodecRegistry**: Registry for content type codecs
- **XMTPPush**: Push notification management

### Dependencies
The SDK uses libxmtp native library through JNI bindings for core XMTP protocol implementation. The project uses Protocol Buffers for message serialization and includes support for various content types through a codec system.

## Development Notes

### Platform Requirements
- **Android**: Minimum SDK 23, Target SDK 35
- **Kotlin**: Version 2.0.0
- **Java**: Version 17
- **Android Gradle Plugin**: Version 8.4.0

### Testing
The project includes comprehensive test coverage:
- Unit tests in `library/src/test/`
- Instrumented tests in `library/src/androidTest/`
- Example app tests in `example/src/androidTest/`

Tests cover:
- Client functionality and authentication
- Group operations and MLS
- DM operations
- Content types and codecs
- Cryptographic operations
- Push notifications

### Building & Publishing
- Uses Maven Central for distribution
- Requires signing for release builds
- Publishes to Sonatype OSSRH
- Documentation published to GitHub Pages

### Code Review
- Pull requests are automatically reviewed by Claude AI via GitHub Actions
- Reviews focus on code quality, security, performance, and adherence to Kotlin/Android best practices
- Requires `CLAUDE_CODE_OAUTH_TOKEN` secret to be configured in repository settings

### Important Files
- `build.gradle`: Root build configuration
- `library/build.gradle`: Library module configuration with publishing setup
- `example/build.gradle`: Example app configuration
- `settings.gradle`: Project structure configuration
- `library/src/main/AndroidManifest.xml`: Library manifest
- Native libxmtp binaries are included for ARM64 and x86_64 architectures

The SDK supports XMTP V3 only (V2 deprecated as of May 2025) with full MLS support for secure group messaging. The project uses ktlint for code formatting and includes comprehensive documentation generated with Dokka.