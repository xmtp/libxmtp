# xmtp-android

[![Test](https://github.com/xmtp/libxmtp/actions/workflows/test-android.yml/badge.svg)](https://github.com/xmtp/libxmtp/actions/workflows/test-android.yml)
[![Lint](https://github.com/xmtp/libxmtp/actions/workflows/lint-android.yml/badge.svg)](https://github.com/xmtp/libxmtp/actions/workflows/lint-android.yml)

`xmtp-android` provides a Kotlin implementation of an XMTP message API client for use with Android apps.

Use `xmtp-android` to build with XMTP to send messages between blockchain accounts, including DMs, notifications, announcements, and more.

> **Note:** This SDK is now part of the [libxmtp monorepo](https://github.com/xmtp/libxmtp). For issues and contributions, please use the main repository.

## Documentation

To learn how to use the XMTP Android SDK, see [Get started with the XMTP Android SDK](https://docs.xmtp.org/sdks/android).

## SDK reference

Access the [Kotlin client SDK reference documentation](https://xmtp.github.io/xmtp-android/).

## Example app

Use the [XMTP Android quickstart app](./example) as a tool to start building an app with XMTP. This basic messaging app has an intentionally unopinionated UI to help make it easier for you to build with.

To learn about example app push notifications, see [Enable the quickstart app to send push notifications](library/src/main/java/org/xmtp/android/library/push/README.md).

## Install from Maven Central

You can find the latest package version on [Maven Central](https://central.sonatype.com/artifact/org.xmtp/android/3.0.0/versions).

```gradle
    implementation 'org.xmtp:android:X.X.X'
```

## Breaking revisions

Because `xmtp-android` is in active development, you should expect breaking revisions that might require you to adopt the latest SDK release to enable your app to continue working as expected.

Breaking revisions in an `xmtp-android` release are described on the [Releases page](https://github.com/xmtp/libxmtp/releases).

## Deprecation

XMTP communicates about deprecations in the [XMTP Community Forums](https://community.xmtp.org/), providing as much advance notice as possible.

Older versions of the SDK will eventually be deprecated, which means:

1. The network will not support and eventually actively reject connections from clients using deprecated versions.
2. Bugs will not be fixed in deprecated versions.

The following table provides the deprecation schedule.

| Announced              | Effective     | Minimum Version | Rationale                                                                                                                                                                  |
|------------------------|---------------|-----------------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| No more support for XMTP V2 | May 1, 2025 | >=4.0.3 | In a move toward better security with MLS and the ability to decentralize, we will be shutting down XMTP V2 and moving entirely to XMTP V3. To learn more about V2 deprecation, see [XIP-53: XMTP V2 deprecation plan](https://community.xmtp.org/t/xip-53-xmtp-v2-deprecation-plan/867). |

Bug reports, feature requests, and PRs are welcome in accordance with the [libxmtp contribution guidelines](../../CONTRIBUTING.md).

## Development Setup

### Prerequisites

This SDK is part of the libxmtp monorepo and uses Nix for reproducible builds.

1. Install [Determinate Nix](https://docs.determinate.systems/):
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf -L https://install.determinate.systems/nix | sh -s -- install
   ```

2. (Optional) Install [direnv](https://direnv.net/) for automatic environment setup:
   ```bash
   # After installing direnv, allow this directory
   direnv allow
   ```

### Building

```bash
# Enter the Android development shell
nix develop ../../#android

# Build native bindings (.so files + Kotlin bindings)
./dev/bindings

# Build the full SDK
./dev/build
```

### Code Quality

```bash
# Format code
./gradlew spotlessApply

# Run lint checks
./gradlew :library:lintDebug
```

### Testing

```bash
# Run unit tests
./gradlew library:testDebug

# Run instrumented tests (requires emulator or device)
./gradlew connectedCheck
```
