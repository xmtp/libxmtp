# Release workflow wiring

For operator commands, see [Creating a Release](../docs/create-a-release.md).

SDK release tags use `GH_APP_ID` and `GH_APP_PK` for the release App.
The App installation must grant Contents write and Workflows write.
App tokens are available only in isolated permission and tag push jobs.
These jobs do not run SDK or release tools. The tag push job imports a Git
bundle into a new bare repository.
Resolve the requested ref before SDK code runs. All SDK jobs use that fixed
source SHA for checkout, build, and tag checks. `release.yml` freezes the
source in `validate` and computes every selected SDK version there, once,
from the requested ref (for dev classification) and one run timestamp. It
passes the source SHA and the version to each SDK workflow; the backend
computes its own version after it pins its source. Only a direct dispatch of
`release-agent-sdk.yml` or `release-cli.yml` computes its version in the SDK
workflow. The version CLI gets its hash from the pinned HEAD.
Each SDK workflow runs `check-release-push.yml` once before it builds.
`npm-publish.yml` takes the checked `source-sha` from its caller. Its publish
job authorizes itself as well: a real publish runs in the `release`
environment (for its branch policy; the job reads no App secret) and checks
that the source is on main or a release/** branch, so another branch's
workflow cannot call it to publish.
Android and npm tags must point to that SHA. iOS can use one direct
release commit
that changes only the iOS release files. The push job checks this before it
creates its token.
Npm dry runs resolve the source but do not create an App token or push a tag.
`just test dev` checks the tag transfer with a local Git HTTP remote.
`check-release-push.yml` refuses a source commit that is not on `main`
or a `release/**` branch; dry runs skip that check.
There is no scheduled release. A backend release also builds the xdbg image
and publishes it with the backend version as its tag (`release-xdbg` in
`release.yml`). Nothing deploys it.

## Android publication

`release-android.yml` uses the `release` environment for `OSSRH_USERNAME`,
`OSSRH_TOKEN`, `OSSRH_GPG_SECRET_KEY`, and `OSSRH_GPG_SECRET_KEY_PASSWORD`.
The username/token must be a Central Portal user token pair; old OSSRH account
credentials do not work, and no `MAVEN_PROFILE_ID` is needed. GitHub Packages
uses the publish job's `GITHUB_TOKEN`; the caller grants `packages: write`.

Gradle prepares signed `org.xmtp:android` files with
`:library:publishReleasePublicationToReleaseBundleRepository`. The workflow
retains that repository; the `publisher` Gradle project uploads the same files
without rebuilding or signing again. Both registries use Gradle's Maven
publisher. The GitHub Packages job (`xmtp/libxmtp`) must succeed before Maven
Central starts; the Nexus plugin closes and releases Central staging. The tag
job waits for Central. Bundle, tag, and APK artifacts are retained for 30 days.

Use **Re-run failed jobs** to avoid repeating successful registry jobs.
Interrupted or ambiguous uploads may need manual inspection and cleanup;
partial uploads have no automatic recovery guarantee. Never rerun a successful
Central publication for the same version.

## Android testing APK

`release-android.yml` builds the example with
`dev/nix-shell 'just example-android apk'` before Maven publication.
After the isolated tag push, it attaches the test-signed
universal APK to the matching GitHub Release for every release type. Keep the
APK upload job dependent on the tag push and use `--verify-tag` on creation.
The APK build needs no signing secret. Its Nix derivation uses the public AOSP
testing key. The `messenger-build` job in `test-example-android.yml` builds that
derivation first; it uses its own pinned Gradle downloads, not the restored
Gradle User Home, and no backend.
