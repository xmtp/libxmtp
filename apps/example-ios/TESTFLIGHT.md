# Share XMTP Messenger with TestFlight

Use **Release** for the first team build. Messaging works with push notifications off. Use **Push** when you also want to test production APNs. The app supports iPhone and iPad running iOS or iPadOS **26 or later**.

## 1. Prepare the team and server

You need a Mac with supported Xcode, the repository's Nix environment, and an Apple Developer Program team. Sign in to that team in Xcode. Upload access requires an App Store Connect Account Holder, Admin, App Manager, or Developer role. An Account Holder or Admin can register the App ID. An Account Holder, Admin, or App Manager can create the app record and arrange external testing. A free Personal Team is not sufficient for TestFlight. Check Apple's [current upload requirements](https://developer.apple.com/help/app-store-connect/manage-builds/upload-builds/) and [distribution setup](https://developer.apple.com/documentation/xcode/preparing-your-app-for-distribution).

Give testers a reachable **HTTPS backend URL** with a valid certificate. For file sharing, attachment upload and download URLs must also be reachable from their devices over HTTPS. A loopback address on a phone points to that phone, not your Mac. The local simulator backend and S3 addresses are development fixtures. Xcode launch environment variables do not carry into TestFlight installations.

Testers enter the URL on **Join a server**. The **Access token** field appears when the server requires it. Share credentials privately. Keep the backend and attachment storage available throughout testing.

## 2. Set the app identity and signing

Choose a unique bundle identifier that your team owns, such as `com.yourcompany.xmtpMessenger`. The repository default is `org.xmtp.example.ios`; use it only if your team owns it. Register an explicit App ID with the same bundle identifier. See [Register an App ID](https://developer.apple.com/help/account/identifiers/register-an-app-id/).

Open `apps/example-ios/ExampleApp.xcodeproj`. Select **ExampleApp → Signing & Capabilities**, enable **Automatically manage signing**, and select your paid developer team. Set the target's bundle identifier to your registered value. Xcode can create distribution signing assets during upload. See [Prepare an app for distribution](https://developer.apple.com/documentation/xcode/preparing-your-app-for-distribution).

Set **Version** (`MARKETING_VERSION`) to `0.1.0` and **Build** (`CURRENT_PROJECT_VERSION`) to `1`, or your own values. This project does not commit a team, version, or build number. Increase Build for each upload.

You can save `DEVELOPMENT_TEAM = YOUR_TEAM_ID` in gitignored `apps/example-ios/Config/Local.xcconfig`. Set the bundle identifier and version/build in the target's build settings or the archive command below. `Base.xcconfig` defines its default bundle identifier after including `Local.xcconfig`, so a bundle override in that local file alone is overwritten.

In **App Store Connect → Apps → + → New App**, choose iOS, the same bundle identifier, a name, primary language, and unique SKU. See [Add a new app](https://developer.apple.com/help/app-store-connect/create-an-app-record/add-a-new-app/).

## 3. Generate device SDK slices and archive

From the repository root:

```sh
NIX_DEVSHELL=ios dev/nix-shell 'sdks/ios/dev/bindings --release'
```

The first build can take time. The normal simulator build and host-test recipes replace these artifacts with the fast SDK framework. Run the release command again after those recipes and before archiving.

In Xcode, select the **ExampleApp** scheme and **Any iOS Device (arm64)**, or the current generic iOS device destination. In **Product → Scheme → Edit Scheme → Archive**, choose **Release**. The shared scheme already defaults to Release. Select **Product → Archive** and resolve build or signing diagnostics.

For a command-line archive:

```sh
NIX_DEVSHELL=ios dev/nix-shell 'env -u LD xcodebuild archive -project apps/example-ios/ExampleApp.xcodeproj -scheme ExampleApp -configuration Release -destination "generic/platform=iOS" -archivePath apps/example-ios/.build/XMTPMessenger.xcarchive DEVELOPMENT_TEAM=YOUR_TEAM_ID PRODUCT_BUNDLE_IDENTIFIER=YOUR_REGISTERED_BUNDLE_ID MARKETING_VERSION=0.1.0 CURRENT_PROJECT_VERSION=1 SWIFT_SUPPRESS_WARNINGS=NO'
```

Replace the signing placeholders. Keep the team, bundle identifier, Version, and Build consistent with the app record. If signing assets are missing, configure automatic signing in Xcode before retrying.

## 4. Validate and upload

Open **Window → Organizer → Archives**. Select the archive, then **Distribute App → App Store Connect**. Review the signing summary, validate, and upload. Choose regular App Store Connect distribution if you may invite people without App Store Connect access. **TestFlight Internal Only** restricts that build to internal groups. See [Apple's distribution workflow](https://developer.apple.com/documentation/xcode/distributing-your-app-for-beta-testing-and-releases) and [internal-only restrictions](https://developer.apple.com/help/app-store-connect/test-a-beta-version/add-internal-testers/).

Wait for processing under the app's **TestFlight** tab. Resolve validation or processing errors. Review the archive's privacy report and dependency declarations. The app includes a privacy manifest for local preferences and owned file metadata; it does not replace your data-collection disclosures. See [Privacy manifest files](https://developer.apple.com/documentation/bundleresources/privacy-manifest-files).

Complete Apple's encryption/export-compliance questions for MLS and CryptoKit use. Determine the applicable requirements and provide any requested documentation rather than assuming an exemption. See [Export compliance](https://developer.apple.com/help/app-store-connect/manage-app-information/overview-of-export-compliance/).

## 5. Invite the team

For teammates with eligible App Store Connect access, create an **Internal Testing** group, add users, and attach the processed build. Internal testing supports up to 100 users. See [Add internal testers](https://developer.apple.com/help/app-store-connect/test-a-beta-version/add-internal-testers/).

For teammates without App Store Connect access, first create an internal group, then create an **External Testing** group. Add the build and submit it for TestFlight App Review when requested. Complete **Test Information**, including the beta description, feedback email, review contact, backend URL, and reviewer access instructions. After approval, invite teammates by email or enable a public link. See [Test information](https://developer.apple.com/help/app-store-connect/test-a-beta-version/provide-test-information/) and [Invite external testers](https://developer.apple.com/help/app-store-connect/test-a-beta-version/invite-external-testers/).

Testers install Apple's TestFlight app, accept the invitation, install XMTP Messenger, and join the shared server. Ask two teammates to exchange a message and a file to confirm the device/server setup. Use TestFlight feedback for screenshots and bug reports.

## 6. Ship updates

Keep the same bundle identifier and app record. Increase Build for each upload; change Version for a new version series. For example, follow `0.1.0 (1)` with `0.1.0 (2)`, then `0.2.0 (3)`. Generate the release SDK again if needed, archive, validate, upload, and attach the processed build to the tester groups. Fill in **What to Test** with the changes. External builds may require another review.

Testers receive updates through TestFlight. Each build expires after **90 days**, so distribute a replacement before expiry. See [TestFlight overview](https://developer.apple.com/help/app-store-connect/test-a-beta-version/testflight-overview/).

## Optional: production push notifications

Choose **Push** for Archive's build configuration and enable Push Notifications for the registered App ID. This configuration adds `PUSH_ENABLED`, push entitlements, and the remote-notification background mode. Use App Store Connect distribution signing. The checked-in entitlement is for development; Xcode derives the final APNs environment from the provisioning profile. Verify that the distributed build has `aps-environment = production`, which TestFlight requires. See [APNs environment](https://developer.apple.com/documentation/bundleresources/entitlements/aps-environment).

Configure the backend's `[push.apns]` with the APNs signing key, key ID, developer team ID, your exact bundle ID, and `environment = "production"`. Keep APNs keys on the backend. The local HTTP-push-only backend cannot deliver APNs alerts.

Before sharing the Push build broadly, verify delivery and taps on physical devices, including mute, block, and sign-out. Simulator and host tests do not prove production APNs delivery. Distribution signing, a device archive, App Store Connect processing, and TestFlight installation still require your team's account and device setup.
