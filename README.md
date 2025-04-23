# xmtp-android

![Test](https://github.com/xmtp/xmtp-android/actions/workflows/test.yml/badge.svg) ![Lint](https://github.com/xmtp/xmtp-android/actions/workflows/lint.yml/badge.svg)

`xmtp-android` provides a Kotlin implementation of an XMTP message API client for use with Android apps.

Use `xmtp-android` to build with XMTP to send messages between blockchain accounts, including DMs, notifications, announcements, and more.

To keep up with the latest SDK developments, see the [Issues tab](https://github.com/xmtp/xmtp-android/issues) in this repo.

## Documentation

To learn how to use the XMTP Android SDK and get answers to frequently asked questions, see the [XMTP documentation](https://docs.xmtp.org/).

## SDK reference

Access the [Kotlin client SDK reference documentation](https://xmtp.github.io/xmtp-android/).

## Example app built with `xmtp-android`

Use the [XMTP Android quickstart app](https://github.com/xmtp/xmtp-android/tree/main/example) as a tool to start building an app with XMTP. This basic messaging app has an intentionally unopinionated UI to help make it easier for you to build with.

To learn about example app push notifications, see [Enable the quickstart app to send push notifications](library/src/main/java/org/xmtp/android/library/push/README.md).

## Install from Maven Central

You can find the latest package version on [Maven Central](https://central.sonatype.com/artifact/org.xmtp/android/3.0.0/versions).

```gradle
    implementation 'org.xmtp:android:X.X.X'
```

## 🏗 Breaking revisions

Because `xmtp-android` is in active development, you should expect breaking revisions that might require you to adopt the latest SDK release to enable your app to continue working as expected.

Breaking revisions in an `xmtp-android` release are described on the [Releases page](https://github.com/xmtp/xmtp-android/releases).

## Deprecation

XMTP communicates about deprecations in the [XMTP Community Forums](https://community.xmtp.org/), providing as much advance notice as possible.

Older versions of the SDK will eventually be deprecated, which means:

1. The network will not support and eventually actively reject connections from clients using deprecated versions.
2. Bugs will not be fixed in deprecated versions.

The following table provides the deprecation schedule.

| Announced              | Effective     | Minimum Version | Rationale                                                                                                                                                                  |
|------------------------|---------------|-----------------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| No more support for XMTP V2 | May 1, 2025 | >=4.0.3 | In a move toward better security with MLS and the ability to decentralize, we will be shutting down XMTP V2 and moving entirely to XMTP V3. To learn more about V2 deprecation, see [XIP-53: XMTP V2 deprecation plan](https://community.xmtp.org/t/xip-53-xmtp-v2-deprecation-plan/867). To learn how to upgrade, see [xmtp-android v4.0.3](https://github.com/xmtp/xmtp-android/releases/tag/4.0.3). For reference, you can view the [legacy branch](https://github.com/xmtp/xmtp-android/tree/xmtp-legacy). |

Bug reports, feature requests, and PRs are welcome in accordance with [XMTP contribution guidelines](https://github.com/xmtp/xmtp-android/blob/main/CONTRIBUTING.md).
