# Android Release Infrastructure Design

Add Android SDK release support to the monorepo release infrastructure, mirroring the iOS release pattern.

## Overview

- **Version source**: `sdks/android/gradle.properties` (read/write by release-tools)
- **Version format**: `{base}-dev.{sha}` for dev, `{base}-rcN` for RC, `{base}` for final
- **Build process**: Single stage - Nix builds bindings, Gradle publishes to Maven/Sonatype
- **Tagging**: All releases tagged as `android-{version}`
- **Commits**: Only final releases commit the version bump; dev/RC modify locally

## Release Tools Changes

### `dev/release-tools/src/types.ts`

Add Android to the SDK enum:

```typescript
export enum Sdk {
  Ios = "ios",
  Android = "android",
}
```

### `dev/release-tools/src/lib/manifest.ts`

Add Gradle properties manifest provider:

```typescript
const GRADLE_VERSION_REGEX = /^version=(.+)$/m;

export function readGradlePropertiesVersion(propsPath: string): string {
  const content = fs.readFileSync(propsPath, "utf-8");
  const match = content.match(GRADLE_VERSION_REGEX);
  if (!match) {
    throw new Error(`Could not find version= in ${propsPath}`);
  }
  return match[1];
}

export function writeGradlePropertiesVersion(
  propsPath: string,
  version: string,
): void {
  let content = fs.readFileSync(propsPath, "utf-8");
  if (GRADLE_VERSION_REGEX.test(content)) {
    content = content.replace(GRADLE_VERSION_REGEX, `version=${version}`);
  } else {
    content += `\nversion=${version}\n`;
  }
  fs.writeFileSync(propsPath, content);
}

export function createGradlePropertiesManifestProvider(
  relativePath: string,
): ManifestProvider {
  return {
    readVersion: (repoRoot) =>
      readGradlePropertiesVersion(path.join(repoRoot, relativePath)),
    writeVersion: (repoRoot, version) =>
      writeGradlePropertiesVersion(path.join(repoRoot, relativePath), version),
  };
}
```

### `dev/release-tools/src/lib/sdk-config.ts`

Add Android config:

```typescript
[Sdk.Android]: {
  name: "Android",
  manifestPath: "sdks/android/gradle.properties",
  tagPrefix: "android-",
  artifactTagSuffix: "-libxmtp",
  manifest: createGradlePropertiesManifestProvider("sdks/android/gradle.properties"),
},
```

## Gradle Changes

### `sdks/android/gradle.properties`

Add version property:

```properties
version=4.9.0
```

### `sdks/android/build.gradle`

Remove the environment variable line:

```groovy
// Remove this line:
version = System.getenv("RELEASE_VERSION")
```

The `version` property will be read automatically from `gradle.properties`.

### `sdks/android/library/build.gradle`

Change from:

```groovy
version = System.getenv("RELEASE_VERSION")
```

To:

```groovy
version = rootProject.version
```

## GitHub Workflows

### `.github/workflows/dev-release.yml`

Add Android input alongside iOS:

```yaml
on:
  workflow_dispatch:
    inputs:
      branch:
        description: "Branch to release from (defaults to the branch the workflow is run from)"
        required: false
        type: string
      ios:
        description: "Release iOS SDK"
        required: false
        type: boolean
        default: false
      android:
        description: "Release Android SDK"
        required: false
        type: boolean
        default: false

jobs:
  release-ios:
    if: inputs.ios
    uses: ./.github/workflows/release-ios.yml
    # ... existing config

  release-android:
    if: inputs.android
    uses: ./.github/workflows/release-android.yml
    with:
      release-type: dev
      ref: ${{ inputs.branch || github.ref }}
    secrets: inherit

  notify:
    needs: [release-ios, release-android]
    # Update notification to include Android result
```

### `.github/workflows/release-android.yml` (new file)

```yaml
name: Release Android SDK

on:
  workflow_call:
    inputs:
      release-type:
        required: true
        type: string
        description: "dev, rc, or final"
      rc-number:
        required: false
        type: number
        description: "RC number (required for rc releases)"
      ref:
        required: true
        type: string
        description: "Git ref to build from"
    outputs:
      version:
        description: "The published version string"
        value: ${{ jobs.publish.outputs.version }}

jobs:
  compute-version:
    runs-on: ubuntu-latest
    outputs:
      version: ${{ steps.version.outputs.version }}
    steps:
      - uses: actions/checkout@v6
        with:
          ref: ${{ inputs.ref }}
      - uses: ./.github/actions/setup-node
      - name: Install release tools
        working-directory: dev/release-tools
        run: yarn install
      - name: Compute version
        id: version
        working-directory: dev/release-tools
        run: |
          if [ "${{ inputs.release-type }}" = "rc" ]; then
            VERSION=$(yarn cli compute-version --sdk android --release-type rc --rc-number ${{ inputs.rc-number }})
          elif [ "${{ inputs.release-type }}" = "dev" ]; then
            VERSION=$(yarn cli compute-version --sdk android --release-type dev)
          else
            VERSION=$(yarn cli compute-version --sdk android --release-type final)
          fi
          echo "version=$VERSION" >> "$GITHUB_OUTPUT"

  publish:
    needs: [compute-version]
    runs-on: warp-ubuntu-latest-arm64-8x
    outputs:
      version: ${{ needs.compute-version.outputs.version }}
    steps:
      - uses: actions/checkout@v6
        with:
          ref: ${{ inputs.ref }}
          fetch-depth: 0
      - uses: ./.github/actions/setup-node
      - uses: ./.github/actions/setup-nix
        with:
          github-token: ${{ github.token }}
          cachix-auth-token: ${{ secrets.CACHIX_AUTH_TOKEN }}
      
      - name: Install release tools
        working-directory: dev/release-tools
        run: yarn install
      
      - name: Set version in gradle.properties
        working-directory: dev/release-tools
        env:
          VERSION: ${{ needs.compute-version.outputs.version }}
        run: yarn cli set-manifest-version --sdk android --version "$VERSION"
      
      - name: Build Android bindings
        run: ./sdks/android/dev/bindings
      
      - name: Configure JDK
        uses: actions/setup-java@v4
        with:
          distribution: 'adopt'
          java-version: '17'
      
      - name: Setup Gradle
        uses: gradle/actions/setup-gradle@v3
      
      - name: Build and publish
        working-directory: sdks/android
        env:
          MAVEN_USERNAME: ${{ secrets.OSSRH_USERNAME }}
          MAVEN_PASSWORD: ${{ secrets.OSSRH_TOKEN }}
          SIGN_KEY: ${{ secrets.OSSRH_GPG_SECRET_KEY }}
          SIGN_PASSWORD: ${{ secrets.OSSRH_GPG_SECRET_KEY_PASSWORD }}
          MAVEN_PROFILE_ID: ${{ secrets.MAVEN_PROFILE_ID }}
        run: ./gradlew build publishToSonatype closeAndReleaseSonatypeStagingRepository
      
      - name: Commit and tag (final releases only)
        if: inputs.release-type == 'final'
        env:
          VERSION: ${{ needs.compute-version.outputs.version }}
        run: |
          TAG="android-${VERSION}"
          git config user.name "github-actions[bot]"
          git config user.email "github-actions[bot]@users.noreply.github.com"
          git add sdks/android/gradle.properties
          git commit -m "release: Android SDK $VERSION [skip ci]"
          git tag "$TAG"
          git push origin HEAD "$TAG"
      
      - name: Tag only (dev/rc releases)
        if: inputs.release-type != 'final'
        env:
          VERSION: ${{ needs.compute-version.outputs.version }}
        run: |
          TAG="android-${VERSION}"
          git config user.name "github-actions[bot]"
          git config user.email "github-actions[bot]@users.noreply.github.com"
          git tag "$TAG"
          git push origin "$TAG"
```

## File Summary

### Files to modify

| File | Change |
|------|--------|
| `dev/release-tools/src/types.ts` | Add `Sdk.Android` to enum |
| `dev/release-tools/src/lib/manifest.ts` | Add Gradle properties read/write functions |
| `dev/release-tools/src/lib/sdk-config.ts` | Add Android SDK config |
| `sdks/android/gradle.properties` | Add `version=4.9.0` property |
| `sdks/android/build.gradle` | Remove `System.getenv("RELEASE_VERSION")` |
| `sdks/android/library/build.gradle` | Change to `version = rootProject.version` |
| `.github/workflows/dev-release.yml` | Add `android` input and job |

### Files to create

| File | Purpose |
|------|---------|
| `.github/workflows/release-android.yml` | Reusable Android release workflow |
