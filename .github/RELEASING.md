# Release workflow wiring

For operator commands, see [Creating a Release](../docs/create-a-release.md).

SDK release tags use `GH_APP_ID` and `GH_APP_PK` for the release App.
The App installation must grant Contents write and Workflows write.
App tokens are available only in isolated permission and tag push jobs.
These jobs do not run SDK or release tools. The tag push job imports a Git
bundle into a new bare repository.
Resolve the requested ref before SDK code runs. All SDK jobs use that fixed
source SHA for checkout, build, and tag checks. `release.yml` freezes the
source and one shared SDK/CLI version in a retained `sdk-release-plan`.
Every builder materializes the plan into Cargo and product metadata before
compilation. All six SDKs, plus CLI when selected, must pass package and installed
consumer validation before any SDK or CLI publication. Product selectors restrict
publication destinations, never this validation barrier. Agent and CLI publication
also select Node and require its exact matching version. Backend version labels
remain independent.

Reusable SDK publishers are internal to `release.yml`. Real publication rejects
any other calling workflow, so invoking a product wrapper cannot bypass the
all-six barrier. Dry builds do not require this publication origin.

Publishers consume retained checked bytes and reject conflicting retries. Node
builds use deterministic local materialization commits for cache and provenance
identities; the frozen plan continues to name the original source. SwiftPM uses a
version-keyed archive with a source/version-bound receipt; publish that archive
before its installable tag. Standalone iOS CocoaPods distribution is retired;
React Native retains its npm-shipped podspec.
Each SDK workflow runs `check-release-push.yml` once before it builds.
`npm-prepare.yml` packages, checks, retains, and previews npm products with
read-only permissions. Build and consumer jobs cannot request publication OIDC
authority through their nested calls. `npm-publish.yml` accepts only retained
checked bytes after the all-six barrier; only its actual publisher job receives
`id-token: write`. It takes the checked `source-sha` from its caller and
authorizes itself as well: a real publish runs in the `release`
environment (for its branch policy; the job reads no App secret) and checks
that the source is on main or a release/** branch, so another branch's
workflow cannot call it to publish.
Android and npm tags must point to that SHA. iOS can use one direct
release commit
that changes only the iOS release files. The push job checks this before it
creates its token.
Npm dry runs resolve the source but do not create an App token or push a tag.
Windows dry runs restore existing build caches but never save new cache entries.
Native preparation validates existing version tags against the frozen source
before reusing them. Android tags must peel to that source. iOS permits the
source or one direct release-only commit; existing generated sources, receipt,
binary tag, and archive must agree with the intended source/version and fresh
build. For matching iOS payloads, preparation retains the existing immutable
archive bytes even if a new ZIP has different container timestamps. These
checks do not skip native builds or installed-consumer validation, and never
replace a published tag or artifact.
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
