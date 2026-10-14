# Share XMTP Messenger with TestFlight

Use **Release** for the first team build. Messaging works without APNs. Use **Push** when the backend also has APNs configured. Devices need iOS or iPadOS 26 or later.

## 1. Prepare the team and server

Use an Apple Developer Program team and an App Store Connect account with upload access. Sign in to that team in Xcode. Check Apple’s [current upload requirements](https://developer.apple.com/help/app-store-connect/manage-builds/upload-builds/) before archiving.

Give testers a reachable **HTTPS** server URL. The simulator’s loopback address points to the tester’s phone on a device; it will not reach your Mac. Testers enter the URL on Join. The Access token field appears when the server requires it. Share credentials privately. Xcode launch environment variables do not carry into a TestFlight installation.

## 2. Set signing and create the app record

Open `apps/example-ios/ExampleApp.xcodeproj`. Select **ExampleApp → Signing & Capabilities**, enable automatic signing and select your team. Use a bundle identifier registered to that team; the repository default is `org.xmtp.example.ios`. Set it in the target’s build settings if your team does not own that identifier.

Set Version (`MARKETING_VERSION`) to `0.1.0` and Build (`CURRENT_PROJECT_VERSION`) to `1`, or your own values. Increase Build for every upload.

In App Store Connect, select **Apps → + → New App**. Choose iOS, the same bundle identifier, a name, primary language and unique SKU. Follow [Apple’s app-record instructions](https://developer.apple.com/help/app-store-connect/create-an-app-record/add-a-new-app/).

## 3. Generate device SDK slices and archive

From the repository root:

```sh
NIX_DEVSHELL=ios dev/nix-shell 'sdks/ios/dev/bindings --release'
```

The first build can take time. Do not run the normal simulator build recipe before archiving: it replaces these artifacts with the fast simulator framework.

In Xcode, select the **ExampleApp** scheme and **Any iOS Device (arm64)**. In **Product → Scheme → Edit Scheme → Archive**, choose **Release**. Then select **Product → Archive**. Resolve build and signing diagnostics before uploading.

For a command-line archive:

```sh
NIX_DEVSHELL=ios dev/nix-shell 'env -u LD xcodebuild archive -project apps/example-ios/ExampleApp.xcodeproj -scheme ExampleApp -configuration Release -destination "generic/platform=iOS" -archivePath apps/example-ios/.build/XMTPMessenger.xcarchive DEVELOPMENT_TEAM=YOUR_TEAM_ID PRODUCT_BUNDLE_IDENTIFIER=YOUR_REGISTERED_BUNDLE_ID MARKETING_VERSION=0.1.0 CURRENT_PROJECT_VERSION=1 SWIFT_SUPPRESS_WARNINGS=NO'
```

Replace the signing placeholders. You can save `DEVELOPMENT_TEAM = YOUR_TEAM_ID` in gitignored `apps/example-ios/Config/Local.xcconfig`. Set the bundle identifier in the target or command above; Base.xcconfig defines the repository default after including that local file.

## 4. Upload

In **Window → Organizer → Archives**, select the archive, then **Distribute App → App Store Connect**. Validate and upload. Choose regular App Store Connect distribution if you may invite people outside App Store Connect. **TestFlight Internal Only** restricts that build to internal testers. See [Apple’s distribution instructions](https://developer.apple.com/documentation/xcode/distributing-your-app-for-beta-testing-and-releases).

Wait for processing under the app’s **TestFlight** tab. Resolve validation or processing errors. The app includes privacy API declarations for local preferences and owned file metadata. Review the archive’s privacy report and dependency declarations; that declaration does not replace the app’s data-collection disclosures. See [privacy manifests](https://developer.apple.com/documentation/bundleresources/privacy-manifest-files).

Complete the encryption/export-compliance questions for MLS and CryptoKit use. Do not assume a generic exemption. Provide any documentation Apple requests through its [export-compliance workflow](https://developer.apple.com/help/app-store-connect/manage-app-information/overview-of-export-compliance/).

## 5. Invite the team

For teammates with App Store Connect access, create an **Internal Testing** group, add eligible users and attach the processed build. Internal testing supports up to 100 users. See [Add internal testers](https://developer.apple.com/help/app-store-connect/test-a-beta-version/add-internal-testers/).

For teammates without App Store Connect access, create an **External Testing** group. Fill in Test Information and the feedback and review contacts. Include the reachable server URL and reviewer access instructions. Add the build and submit for TestFlight App Review when requested. After approval, invite by email or enable a public link. See [Invite external testers](https://developer.apple.com/help/app-store-connect/test-a-beta-version/invite-external-testers/).

Testers install Apple’s TestFlight app, accept the invitation, install XMTP Messenger and join your shared server. Builds expire after 90 days. Upload a new build before expiry. See [TestFlight overview](https://developer.apple.com/help/app-store-connect/test-a-beta-version/testflight-overview/).

## Optional notifications

Choose **Push** for Archive’s build configuration. Enable Push Notifications for the registered App ID. Use distribution signing and check that the exported app’s `aps-environment` entitlement is `production`. TestFlight uses production APNs. See [Apple’s APNs environment reference](https://developer.apple.com/documentation/bundleresources/entitlements/aps-environment).

Configure the backend’s `[push.apns]` for the same bundle ID and production APNs. Keep keys on the backend. The local HTTP-push-only backend cannot deliver APNs alerts. Before team-wide sharing, verify delivery and taps on physical devices, including mute, block and sign-out. Simulator compilation does not prove device delivery.
