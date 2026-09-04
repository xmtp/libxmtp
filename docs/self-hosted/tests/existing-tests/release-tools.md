# Release-tool test index

[← Test inventory](../existing-tests.md) · [Requirements](../existing-requirements.md)

| File | Qualified test name | Form, gates, and cases | Requirements |
| --- | --- | --- | --- |
| `dev/release-tools/tests/version.test.ts` | `filterAndSortTags :: filters tags by prefix and sorts descending` | Vitest sync | `RELEASE-REQ-001` |
| `dev/release-tools/tests/version.test.ts` | `filterAndSortTags :: includes prerelease tags when flag is set` | Vitest sync; stable, rc, and dev | `RELEASE-REQ-001` |
| `dev/release-tools/tests/version.test.ts` | `filterAndSortTags :: excludes prerelease tags by default` | Vitest sync | `RELEASE-REQ-001` |
| `dev/release-tools/tests/version.test.ts` | `filterAndSortTags :: returns empty array when no tags match` | Vitest sync | `RELEASE-REQ-001` |
| `dev/release-tools/tests/version.test.ts` | `filterAndSortTags :: excludes artifact tags ending in suffix` | Vitest sync | `RELEASE-REQ-001` |
| `dev/release-tools/tests/version.test.ts` | `normalizeVersion :: normalizeVersion(%s) => %s` | Vitest it.each; six stable, dev, rc, build, rc-plus-build, and nightly cases | `RELEASE-REQ-002` |
| `dev/release-tools/tests/version.test.ts` | `normalizeVersion :: throws on invalid input: %s` | Vitest it.each; invalid and empty | `RELEASE-REQ-002` |
| `dev/release-tools/tests/version.test.ts` | `computeVersion :: returns base version for final release` | Vitest sync | `RELEASE-REQ-003` |
| `dev/release-tools/tests/version.test.ts` | `computeVersion :: appends rc suffix for rc release` | Vitest sync; rc1 | `RELEASE-REQ-003` |
| `dev/release-tools/tests/version.test.ts` | `computeVersion :: appends dev suffix with short sha` | Vitest sync; legacy dev | `RELEASE-REQ-003` |
| `dev/release-tools/tests/version.test.ts` | `computeVersion :: throws if rc release has no rcNumber` | Vitest sync negative | `RELEASE-REQ-003` |
| `dev/release-tools/tests/version.test.ts` | `computeVersion :: throws if dev release has no shortSha` | Vitest sync negative | `RELEASE-REQ-003` |
| `dev/release-tools/tests/version.test.ts` | `computeVersion :: appends nightly suffix with timestamp and short sha` | Vitest sync | `RELEASE-REQ-003`, `RELEASE-REQ-004` |
| `dev/release-tools/tests/version.test.ts` | `computeVersion :: throws if nightly release has no timestamp` | Vitest sync negative | `RELEASE-REQ-003` |
| `dev/release-tools/tests/version.test.ts` | `computeVersion :: throws if nightly release has no shortSha` | Vitest sync negative | `RELEASE-REQ-003` |
| `dev/release-tools/tests/version.test.ts` | `computeVersion :: nightly versions sort lexicographically by timestamp` | Vitest sync; three dates; lexical and SemVer order | `RELEASE-REQ-004` |
| `dev/release-tools/tests/version.test.ts` | `computeVersion :: nightly is a valid semver prerelease` | Vitest sync | `RELEASE-REQ-004` |
| `dev/release-tools/tests/version.test.ts` | `unified pre.* prerelease ordering :: formats main-cut nightly as pre.<ts>.nightly.<sha>` | Vitest sync | `RELEASE-REQ-004` |
| `dev/release-tools/tests/version.test.ts` | `unified pre.* prerelease ordering :: formats main-cut dev as pre.<ts>.dev.<sha>` | Vitest sync; fromMain true | `RELEASE-REQ-004` |
| `dev/release-tools/tests/version.test.ts` | `unified pre.* prerelease ordering :: keeps branch-cut dev on the legacy shape` | Vitest sync | `RELEASE-REQ-004` |
| `dev/release-tools/tests/version.test.ts` | `unified pre.* prerelease ordering :: orders the unified timeline by timestamp across channels` | Vitest sync; later dev above earlier nightly | `RELEASE-REQ-004` |
| `dev/release-tools/tests/version.test.ts` | `unified pre.* prerelease ordering :: orders a later nightly above an earlier dev` | Vitest sync | `RELEASE-REQ-004` |
| `dev/release-tools/tests/version.test.ts` | `unified pre.* prerelease ordering :: ignores the timestamp for a branch-cut dev` | Vitest sync | `RELEASE-REQ-004` |
| `dev/release-tools/tests/version.test.ts` | `unified pre.* prerelease ordering :: sorts legacy shapes below all pre.* and rc above` | Vitest sync | `RELEASE-REQ-004` |
| `dev/release-tools/tests/version.test.ts` | `unified pre.* prerelease ordering :: getTimestamp returns UTC YYYYMMDDHHMMSS` | Vitest sync; fake clock at subsecond instant | `RELEASE-REQ-005` |
| `dev/release-tools/tests/version.test.ts` | `unified pre.* prerelease ordering :: orders same-minute runs by seconds, not by channel name` | Vitest sync; seconds 00, 30, and 59 | `RELEASE-REQ-004` |
| `dev/release-tools/tests/version.test.ts` | `unified pre.* prerelease ordering :: keeps the widened stamp a numeric semver identifier` | Vitest sync; safe integer | `RELEASE-REQ-004`, `RELEASE-REQ-005` |
| `dev/release-tools/tests/version.test.ts` | `unified pre.* prerelease ordering :: requires timestamp for main-cut builds` | Vitest sync; dev and nightly negatives | `RELEASE-REQ-003`, `RELEASE-REQ-004` |
| `dev/release-tools/tests/version.test.ts` | `validateTimestamp :: passes an absent stamp through as the now-fallback signal` | Vitest sync; undefined and empty | `RELEASE-REQ-005` |
| `dev/release-tools/tests/version.test.ts` | `validateTimestamp :: accepts %s (%s)` | Vitest it.each; four real instants including leap day and year edges | `RELEASE-REQ-005` |
| `dev/release-tools/tests/version.test.ts` | `validateTimestamp :: rejects the wrong shape: %s` | Vitest it.each; five wrong-length, nondigit, and space cases | `RELEASE-REQ-005` |
| `dev/release-tools/tests/version.test.ts` | `validateTimestamp :: rejects %s (%s)` | Vitest it.each; nine impossible calendar and time cases | `RELEASE-REQ-005` |
| `dev/release-tools/tests/version.test.ts` | `validateTimestamp :: rejects an all-zero stamp, which would emit unparseable semver` | Vitest sync | `RELEASE-REQ-005` |
| `dev/release-tools/tests/version.test.ts` | `validateTimestamp :: rejects the leading-zero stamp %s` | Vitest it.each; years 0999 and 0001 | `RELEASE-REQ-005` |
| `dev/release-tools/tests/version.test.ts` | `validateTimestamp :: accepts every stamp getTimestamp mints` | Vitest sync; loop over five fixed instants | `RELEASE-REQ-005` |
| `dev/release-tools/tests/version.test.ts` | `validateTimestamp :: keeps every accepted stamp a valid semver numeric identifier` | Vitest sync | `RELEASE-REQ-005` |
| `dev/release-tools/tests/sdk-version.test.ts` | `diffBumpKind :: diffBumpKind(%s, %s) => %s` | Vitest it.each; patch, minor, and major | `RELEASE-REQ-006` |
| `dev/release-tools/tests/sdk-version.test.ts` | `diffBumpKind :: throws when the target is not greater than the base` | Vitest sync negative | `RELEASE-REQ-006` |
| `dev/release-tools/tests/sdk-version.test.ts` | `applyBumpKind :: applyBumpKind(%s, %s) => %s` | Vitest it.each; patch, minor, and major | `RELEASE-REQ-006` |
| `dev/release-tools/tests/sdk-version.test.ts` | `resolveSdkVersion :: follows-libxmtp takes the pending number verbatim (final)` | Vitest sync | `RELEASE-REQ-007` |
| `dev/release-tools/tests/sdk-version.test.ts` | `resolveSdkVersion :: independent mirrors the bump kind onto its own base (final)` | Vitest sync | `RELEASE-REQ-007` |
| `dev/release-tools/tests/sdk-version.test.ts` | `resolveSdkVersion :: follows-libxmtp nightly previews the next number` | Vitest sync | `RELEASE-REQ-007` |
| `dev/release-tools/tests/sdk-version.test.ts` | `resolveSdkVersion :: independent nightly previews the next number on its own base` | Vitest sync | `RELEASE-REQ-007` |
| `dev/release-tools/tests/sdk-version.test.ts` | `resolveSdkVersion :: nightly requires timestamp and sha` | Vitest sync negative | `RELEASE-REQ-007` |
| `dev/release-tools/tests/sdk-version.test.ts` | `capBumpKind :: clamps a major bump down to the max kind and recomputes the version` | Vitest sync; major to minor | `RELEASE-REQ-008` |
| `dev/release-tools/tests/sdk-version.test.ts` | `capBumpKind :: leaves a bump at or below the cap unchanged` | Vitest sync; minor and patch cases | `RELEASE-REQ-008` |
| `dev/release-tools/tests/sdk-version.test.ts` | `capBumpKind :: a cap above the computed kind is a no-op (never raises a bump)` | Vitest sync; patch under major cap | `RELEASE-REQ-008` |
| `dev/release-tools/tests/sdk-version.test.ts` | `capBumpKind :: clamps major down to patch when capped at patch` | Vitest sync | `RELEASE-REQ-008` |
| `dev/release-tools/tests/commands/compute-version.test.ts` | `compute-version --source-ref / --timestamp :: emits the ordered pre.* shape for a main-cut dev` | Vitest sync declaration; real temporary Git repository | `RELEASE-REQ-009` |
| `dev/release-tools/tests/commands/compute-version.test.ts` | `compute-version --source-ref / --timestamp :: emits the legacy shape without --source-ref` | Real temporary Git repository | `RELEASE-REQ-009` |
| `dev/release-tools/tests/commands/compute-version.test.ts` | `compute-version --source-ref / --timestamp :: treats a branch-cut ref as legacy` | Real temporary Git repository | `RELEASE-REQ-009` |
| `dev/release-tools/tests/commands/compute-version.test.ts` | `compute-version --source-ref / --timestamp :: accepts a bare 'main' as well as refs/heads/main` | Real temporary Git repository | `RELEASE-REQ-009` |
| `dev/release-tools/tests/commands/compute-version.test.ts` | `compute-version --source-ref / --timestamp :: uses the supplied --timestamp verbatim` | Real temporary Git repository; fixed timestamp | `RELEASE-REQ-009` |
| `dev/release-tools/tests/commands/compute-version.test.ts` | `compute-version --source-ref / --timestamp :: rejects a malformed --timestamp` | Real temporary Git repository; 2026 | `RELEASE-REQ-009`, `RELEASE-REQ-005` |
| `dev/release-tools/tests/commands/compute-version.test.ts` | `compute-version --source-ref / --timestamp :: rejects a minute-precision (12-digit) --timestamp` | Real temporary Git repository | `RELEASE-REQ-009`, `RELEASE-REQ-005` |
| `dev/release-tools/tests/commands/compute-version.test.ts` | `compute-version --source-ref / --timestamp :: rejects the impossible calendar stamp %s` | Vitest it.each; month 13, February 30, and all zero | `RELEASE-REQ-009`, `RELEASE-REQ-005` |
| `dev/release-tools/tests/commands/bump-version.test.ts` | `bumpVersion :: bumps patch version` | Vitest sync; temporary podspec | `RELEASE-REQ-010` |
| `dev/release-tools/tests/commands/bump-version.test.ts` | `bumpVersion :: bumps minor version` | Vitest sync; temporary podspec | `RELEASE-REQ-010` |
| `dev/release-tools/tests/commands/bump-version.test.ts` | `bumpVersion :: bumps major version` | Vitest sync; temporary podspec | `RELEASE-REQ-010` |
| `dev/release-tools/tests/commands/bump-version.test.ts` | `bumpVersion :: normalizes %s before bumping %s => %s` | Vitest it.each; dev to patch, rc to minor, and rc-plus-build to major | `RELEASE-REQ-010`, `RELEASE-REQ-002` |
| `dev/release-tools/tests/commands/tag-release.test.ts` | `buildTag :: builds iOS tag with prefix` | Vitest sync | `RELEASE-REQ-011` |
| `dev/release-tools/tests/commands/tag-release.test.ts` | `buildTag :: builds Android tag with prefix` | Vitest sync | `RELEASE-REQ-011` |
| `dev/release-tools/tests/commands/tag-release.test.ts` | `buildTag :: handles dev versions` | Vitest sync; iOS and Android | `RELEASE-REQ-011` |
| `dev/release-tools/tests/commands/tag-release.test.ts` | `buildTag :: handles rc versions` | Vitest sync; iOS and Android | `RELEASE-REQ-011` |
| `dev/release-tools/tests/commands/tag-release.test.ts` | `buildTag :: throws for unknown SDK` | Vitest sync negative | `RELEASE-REQ-011` |
| `dev/release-tools/tests/commands/tag-release.test.ts` | `buildTag :: throws for empty version` | Vitest sync negative | `RELEASE-REQ-011` |
| `dev/release-tools/tests/commands/tag-release.test.ts` | `buildTag :: throws for non-semver version` | Vitest sync; two invalid forms | `RELEASE-REQ-011` |
| `dev/release-tools/tests/git-cliff.test.ts` | `parsePendingFromContext :: uses git-cliff's bump_type and strips the v prefix` | Vitest sync; JSON context | `RELEASE-REQ-022` |
| `dev/release-tools/tests/git-cliff.test.ts` | `parsePendingFromContext :: returns null when nothing is pending (version == previous, bump_type null)` | Vitest sync | `RELEASE-REQ-022` |
| `dev/release-tools/tests/git-cliff.test.ts` | `parsePendingFromContext :: falls back to diffBumpKind when bump_type is absent` | Vitest sync; major | `RELEASE-REQ-022` |
| `dev/release-tools/tests/git-cliff.test.ts` | `parsePendingFromContext :: returns null when the context has no releases` | Vitest sync | `RELEASE-REQ-022` |
| `dev/release-tools/tests/git-cliff.test.ts` | `parsePendingFromContext :: returns null when the first release has no version` | Vitest sync | `RELEASE-REQ-022` |
| `dev/release-tools/tests/git-cliff.test.ts` | `parsePendingFromContext :: throws on unparseable input` | Vitest sync negative | `RELEASE-REQ-022` |
| `dev/release-tools/tests/git-cliff.test.ts` | `parsePendingFromContext :: throws on an invalid computed version` | Vitest sync negative | `RELEASE-REQ-022` |
| `dev/release-tools/tests/git-cliff.test.ts` | `parsePendingFromContext :: throws when bump_type is absent and the computed version is not > lastShipped` | Vitest sync negative | `RELEASE-REQ-022` |
| `dev/release-tools/tests/manifest/cargo.test.ts` | `cargo manifest :: reads the version from Cargo.toml` | Vitest sync filesystem | `RELEASE-REQ-012` |
| `dev/release-tools/tests/manifest/cargo.test.ts` | `cargo manifest :: writes a new version to Cargo.toml` | Vitest sync filesystem | `RELEASE-REQ-012` |
| `dev/release-tools/tests/manifest/cargo.test.ts` | `cargo manifest :: writes dev and rc version suffixes` | Vitest sync; two writes | `RELEASE-REQ-012` |
| `dev/release-tools/tests/manifest/cargo.test.ts` | `cargo manifest :: preserves comments and other content when writing` | Vitest sync | `RELEASE-REQ-012` |
| `dev/release-tools/tests/manifest/cargo.test.ts` | `cargo manifest :: throws if workspace.package section is missing` | Vitest sync negative | `RELEASE-REQ-012` |
| `dev/release-tools/tests/manifest/cargo.test.ts` | `cargo manifest :: throws if version is missing from workspace.package` | Vitest sync negative | `RELEASE-REQ-012` |
| `dev/release-tools/tests/manifest/cargo.test.ts` | `cargo manifest :: throws on write if workspace.package.version pattern not found` | Vitest sync negative | `RELEASE-REQ-012` |
| `dev/release-tools/tests/manifest/cargo.test.ts` | `cargo manifest :: throws for non-existent file` | Vitest sync negative | `RELEASE-REQ-012` |
| `dev/release-tools/tests/manifest/gradle.test.ts` | `gradle properties manifest :: reads and writes versions with various formats` | Vitest sync; stable, dev, and rc | `RELEASE-REQ-013` |
| `dev/release-tools/tests/manifest/gradle.test.ts` | `gradle properties manifest :: reads version at different positions in file` | Vitest sync; first, last, and spaced | `RELEASE-REQ-013` |
| `dev/release-tools/tests/manifest/gradle.test.ts` | `gradle properties manifest :: preserves other content and comments when writing` | Vitest sync | `RELEASE-REQ-013` |
| `dev/release-tools/tests/manifest/gradle.test.ts` | `gradle properties manifest :: appends version if not present` | Vitest sync | `RELEASE-REQ-013` |
| `dev/release-tools/tests/manifest/gradle.test.ts` | `gradle properties manifest :: throws for missing or invalid version lines` | Vitest sync; no line, commented, subproperty, and missing file | `RELEASE-REQ-013` |
| `dev/release-tools/tests/manifest/package-json.test.ts` | `package.json manifest :: reads the version from package.json` | Vitest sync | `RELEASE-REQ-014` |
| `dev/release-tools/tests/manifest/package-json.test.ts` | `package.json manifest :: writes a new version to package.json` | Vitest sync | `RELEASE-REQ-014` |
| `dev/release-tools/tests/manifest/package-json.test.ts` | `package.json manifest :: writes dev and rc version suffixes` | Vitest sync; two writes | `RELEASE-REQ-014` |
| `dev/release-tools/tests/manifest/package-json.test.ts` | `package.json manifest :: preserves other fields when writing` | Vitest sync | `RELEASE-REQ-014` |
| `dev/release-tools/tests/manifest/package-json.test.ts` | `package.json manifest :: preserves 2-space indentation` | Vitest sync; final newline | `RELEASE-REQ-014` |
| `dev/release-tools/tests/manifest/package-json.test.ts` | `package.json manifest :: throws if version field is missing` | Vitest sync negative | `RELEASE-REQ-014` |
| `dev/release-tools/tests/manifest/package-json.test.ts` | `package.json manifest :: throws for non-existent file` | Vitest sync negative | `RELEASE-REQ-014` |
| `dev/release-tools/tests/manifest/podspec.test.ts` | `podspec manifest :: reads the version from a podspec` | Vitest sync | `RELEASE-REQ-015` |
| `dev/release-tools/tests/manifest/podspec.test.ts` | `podspec manifest :: writes a new version to a podspec` | Vitest sync | `RELEASE-REQ-015` |
| `dev/release-tools/tests/manifest/podspec.test.ts` | `podspec manifest :: preserves other content when writing` | Vitest sync | `RELEASE-REQ-015` |
| `dev/release-tools/tests/manifest/podspec.test.ts` | `podspec manifest :: throws if version line is not found` | Vitest sync negative | `RELEASE-REQ-015` |
| `dev/release-tools/tests/commands/set-manifest-version.test.ts` | `setManifestVersion :: sets versions for iOS with various formats` | Vitest sync filesystem; stable and dev | `RELEASE-REQ-016` |
| `dev/release-tools/tests/commands/set-manifest-version.test.ts` | `setManifestVersion :: sets versions for Android with various formats and preserves properties` | Vitest sync filesystem; stable and dev | `RELEASE-REQ-016` |
| `dev/release-tools/tests/commands/set-manifest-version.test.ts` | `setManifestVersion :: returns the version that was set` | Vitest sync; iOS and Android | `RELEASE-REQ-016` |
| `dev/release-tools/tests/commands/set-manifest-version.test.ts` | `setManifestVersion :: throws for an unknown SDK` | Vitest sync negative | `RELEASE-REQ-016` |
| `dev/release-tools/tests/commands/set-dependency-version.test.ts` | `setPackageJsonDependency :: rewrites a portal: spec to a real published version` | Vitest sync filesystem | `RELEASE-REQ-017` |
| `dev/release-tools/tests/commands/set-dependency-version.test.ts` | `setPackageJsonDependency :: leaves all other fields and formatting intact` | Vitest sync filesystem | `RELEASE-REQ-017` |
| `dev/release-tools/tests/commands/set-dependency-version.test.ts` | `setPackageJsonDependency :: can rewrite a semver dep to a nightly version` | Vitest sync filesystem | `RELEASE-REQ-017` |
| `dev/release-tools/tests/commands/set-dependency-version.test.ts` | `setPackageJsonDependency :: throws when the dependency does not exist in dependencies` | Vitest sync negative | `RELEASE-REQ-017` |
| `dev/release-tools/tests/commands/set-dependency-version.test.ts` | `setPackageJsonDependency :: throws when the package.json has no dependencies block` | Vitest sync negative | `RELEASE-REQ-017` |
| `dev/release-tools/tests/devcontainer.test.ts` | `setDevcontainerImage :: converts a build block to a pinned image reference` | Vitest sync JSONC filesystem | `RELEASE-REQ-023` |
| `dev/release-tools/tests/devcontainer.test.ts` | `setDevcontainerImage :: updates an existing image reference in place` | Vitest sync JSONC filesystem | `RELEASE-REQ-023` |
| `dev/release-tools/tests/devcontainer.test.ts` | `setDevcontainerImage :: inserts image between name and runArgs where build used to be` | Vitest sync; key order | `RELEASE-REQ-023` |
| `dev/release-tools/tests/devcontainer.test.ts` | `setDevcontainerImage :: preserves the leading JSONC comment` | Vitest sync | `RELEASE-REQ-023` |
| `dev/release-tools/tests/devcontainer.test.ts` | `setDevcontainerImage :: preserves the original indent width` | Vitest sync; four spaces | `RELEASE-REQ-023` |
| `dev/release-tools/tests/devcontainer.test.ts` | `setDevcontainerImage :: preserves unrelated keys` | Vitest sync | `RELEASE-REQ-023` |
| `dev/release-tools/tests/devcontainer.test.ts` | `setDevcontainerImage :: throws when neither build nor image is present` | Vitest sync negative | `RELEASE-REQ-023` |
| `dev/release-tools/tests/devcontainer.test.ts` | `setDevcontainerImage :: throws on malformed JSONC` | Vitest sync negative | `RELEASE-REQ-023` |
| `dev/release-tools/tests/spm.test.ts` | `updateSpmChecksum :: updates the url and checksum` | Vitest sync Swift-text filesystem | `RELEASE-REQ-024` |
| `dev/release-tools/tests/spm.test.ts` | `updateSpmChecksum :: preserves the local binary target path` | Vitest sync | `RELEASE-REQ-024` |
| `dev/release-tools/tests/spm.test.ts` | `updateSpmChecksum :: preserves the conditional logic` | Vitest sync; asserts only that output contains `useLocalBinary` and `FileManager.default.fileExists` | `RELEASE-REQ-024` |
| `dev/release-tools/tests/spm.test.ts` | `updateSpmChecksum :: handles widely spaced multiline formatting` | Vitest sync | `RELEASE-REQ-024` |
| `dev/release-tools/tests/spm.test.ts` | `updateSpmChecksum :: throws if url pattern is not found` | Vitest sync negative | `RELEASE-REQ-024` |
| `dev/release-tools/tests/classify-notes.test.ts` | `parseFrontmatter :: parses valid frontmatter` | Vitest sync | `RELEASE-REQ-018` |
| `dev/release-tools/tests/classify-notes.test.ts` | `parseFrontmatter :: parses frontmatter with version but no tag` | Vitest sync | `RELEASE-REQ-018` |
| `dev/release-tools/tests/classify-notes.test.ts` | `parseFrontmatter :: returns nulls for missing fields` | Vitest sync | `RELEASE-REQ-018` |
| `dev/release-tools/tests/classify-notes.test.ts` | `parseFrontmatter :: returns nulls when no frontmatter` | Vitest sync | `RELEASE-REQ-018` |
| `dev/release-tools/tests/classify-notes.test.ts` | `parseFrontmatter :: returns nulls for invalid TOML` | Vitest sync | `RELEASE-REQ-018` |
| `dev/release-tools/tests/classify-notes.test.ts` | `parseFrontmatter :: treats non-string values as null` | Vitest sync | `RELEASE-REQ-018` |
| `dev/release-tools/tests/classify-notes.test.ts` | `isEmptyScaffold :: returns true for scaffold with only HTML comments and headers` | Vitest sync | `RELEASE-REQ-019` |
| `dev/release-tools/tests/classify-notes.test.ts` | `isEmptyScaffold :: returns false for file with real content` | Vitest sync | `RELEASE-REQ-019` |
| `dev/release-tools/tests/classify-notes.test.ts` | `isEmptyScaffold :: returns false for file with content below comments` | Vitest sync | `RELEASE-REQ-019` |
| `dev/release-tools/tests/classify-notes.test.ts` | `isEmptyScaffold :: returns true for empty file` | Vitest sync | `RELEASE-REQ-019` |
| `dev/release-tools/tests/classify-notes.test.ts` | `isEmptyScaffold :: returns true for file with only whitespace after stripping` | Vitest sync | `RELEASE-REQ-019` |
| `dev/release-tools/tests/classify-notes.test.ts` | `classifyNoteFiles :: classifies a mixed set of files` | Vitest sync; empty iOS and content Android | `RELEASE-REQ-019` |
| `dev/release-tools/tests/classify-notes.test.ts` | `classifyNoteFiles :: classifies files with version but no tag` | Vitest sync | `RELEASE-REQ-019` |
| `dev/release-tools/tests/classify-notes.test.ts` | `classifyNoteFiles :: skips files with neither version nor tag` | Vitest sync | `RELEASE-REQ-019` |
| `dev/release-tools/tests/classify-notes.test.ts` | `classifyNoteFiles :: skips files with missing sdk` | Vitest sync | `RELEASE-REQ-019` |
| `dev/release-tools/tests/classify-notes.test.ts` | `classifyNoteFiles :: returns all empty when all are scaffolds` | Vitest sync | `RELEASE-REQ-019` |
| `dev/release-tools/tests/classify-notes.test.ts` | `classifyNoteFiles :: returns all content when all have content` | Vitest sync | `RELEASE-REQ-019` |
| `dev/release-tools/tests/classify-notes.test.ts` | `classifyNoteFiles :: returns empty arrays when no files provided` | Vitest sync | `RELEASE-REQ-019` |
| `dev/release-tools/tests/scaffold-and-classify.test.ts` | `scaffold → classify end-to-end :: classifies an unmodified scaffold with tag as empty` | Vitest sync filesystem | `RELEASE-REQ-021` |
| `dev/release-tools/tests/scaffold-and-classify.test.ts` | `scaffold → classify end-to-end :: classifies an unmodified scaffold without tag as empty` | Vitest sync filesystem | `RELEASE-REQ-021` |
| `dev/release-tools/tests/scaffold-and-classify.test.ts` | `scaffold → classify end-to-end :: classifies a modified scaffold with tag as content` | Vitest sync filesystem | `RELEASE-REQ-021` |
| `dev/release-tools/tests/scaffold-and-classify.test.ts` | `scaffold → classify end-to-end :: classifies a modified scaffold without tag as content` | Vitest sync filesystem | `RELEASE-REQ-021` |
| `dev/release-tools/tests/scaffold-and-classify.test.ts` | `scaffold → classify end-to-end :: classifies a mix of empty and modified scaffolds across SDKs` | Vitest sync filesystem; iOS and Android | `RELEASE-REQ-021` |
| `dev/release-tools/tests/scaffold-and-classify.test.ts` | `scaffold → classify end-to-end :: classifies a mix with and without previous tags` | Vitest sync filesystem | `RELEASE-REQ-021` |
| `dev/release-tools/tests/commands/scaffold-notes.test.ts` | `scaffoldNotes :: creates release notes from a since tag` | Vitest sync filesystem; iOS | `RELEASE-REQ-020` |
| `dev/release-tools/tests/commands/scaffold-notes.test.ts` | `scaffoldNotes :: handles no previous tag gracefully` | Vitest sync filesystem; iOS | `RELEASE-REQ-020` |
| `dev/release-tools/tests/sdk-config.test.ts` | `SDK configs :: returns correct config for each SDK` | Vitest sync; eight SDKs | `RELEASE-REQ-027` |
| `dev/release-tools/tests/sdk-config.test.ts` | `SDK configs :: throws for unknown SDK with available options` | Vitest sync negative | `RELEASE-REQ-027` |
| `dev/release-tools/tests/sdk-config.test.ts` | `SDK configs :: has config for all SDK enum values` | Vitest sync loop over enum | `RELEASE-REQ-027` |
| `dev/release-tools/tests/sdk-config.test.ts` | `SDK configs > Libxmtp manifest provider :: reads and writes version via manifest provider` | Vitest sync temporary Cargo manifest | `RELEASE-REQ-027`, `RELEASE-REQ-012` |
| `dev/release-tools/tests/sdk-config.test.ts` | `SDK configs > Node manifest provider :: reads and writes version via manifest provider` | Vitest sync temporary package.json | `RELEASE-REQ-027`, `RELEASE-REQ-014` |
| `dev/release-tools/tests/sdk-config.test.ts` | `SDK configs > WASM manifest provider :: reads and writes version via manifest provider` | Vitest sync temporary package.json | `RELEASE-REQ-027`, `RELEASE-REQ-014` |
| `dev/release-tools/tests/sdk-config.test.ts` | `SDK configs > Android manifest provider :: reads and writes version via manifest provider` | Vitest sync temporary Gradle manifest | `RELEASE-REQ-027`, `RELEASE-REQ-013` |
| `dev/release-tools/tests/sdk-config.test.ts` | `SDK configs :: declares a version track for every SDK` | Vitest sync; follows and independent | `RELEASE-REQ-027` |
| `dev/release-tools/tests/sdk-config.test.ts` | `SDK configs :: declares a release workflow and channels for shippable SDKs` | Vitest sync; iOS nightly | `RELEASE-REQ-027` |
| `dev/release-tools/tests/sdk-config.test.ts` | `SDK configs :: declares notes globs for every SDK` | Vitest sync loop over enum | `RELEASE-REQ-027` |
| `dev/release-tools/tests/commands/list-sdks.test.ts` | `listSdksForChannel :: returns fan-out targets for nightly, excluding the hub` | Vitest sync; seven rows | `RELEASE-REQ-028` |
| `dev/release-tools/tests/commands/list-sdks.test.ts` | `listSdksForChannel :: each row carries the data the fan-out needs` | Vitest sync; iOS row | `RELEASE-REQ-028` |
| `dev/release-tools/tests/cross-test-gate.test.ts` | `evaluateGate :: passes when a completed successful run exists for the exact SHA` | Vitest sync mocked API payload | `RELEASE-REQ-026` |
| `dev/release-tools/tests/cross-test-gate.test.ts` | `evaluateGate :: fails (skip) when no run matches the SHA` | Vitest sync | `RELEASE-REQ-026` |
| `dev/release-tools/tests/cross-test-gate.test.ts` | `evaluateGate :: fails (skip) when the matching run did not succeed` | Vitest sync; failure | `RELEASE-REQ-026` |
| `dev/release-tools/tests/cross-test-gate.test.ts` | `evaluateGate :: fails (skip) when the matching run is not completed` | Vitest sync; in_progress and null | `RELEASE-REQ-026` |
| `dev/release-tools/tests/cross-test-gate.test.ts` | `evaluateGate :: fails (skip) on an empty run list` | Vitest sync | `RELEASE-REQ-026` |
| `dev/release-tools/tests/cross-test-gate.test.ts` | `evaluateGate :: passes when any green run for the SHA exists despite a later failed re-run` | Vitest sync; failure and success | `RELEASE-REQ-026` |
| `dev/release-tools/tests/cross-test-gate.test.ts` | `evaluateGate :: does not pass on a 'skipped' conclusion (only 'success' counts)` | Vitest sync | `RELEASE-REQ-026` |
| `dev/release-tools/tests/cross-test-gate.test.ts` | `evaluateGate :: does not let a green run on a DIFFERENT sha unblock this sha` | Vitest sync | `RELEASE-REQ-026` |
| `dev/release-tools/tests/cross-test-gate.test.ts` | `evaluateGate :: tolerates a missing workflow_runs key (fail-closed)` | Vitest sync | `RELEASE-REQ-026` |
| `dev/release-tools/tests/git.test.ts` | `git helpers > listTags :: returns all tags` | Vitest sync; real temporary Git repository; three tags | `RELEASE-REQ-025` |
| `dev/release-tools/tests/git.test.ts` | `git helpers > listTags :: returns empty array for repo with no tags` | Vitest sync; real Git | `RELEASE-REQ-025` |
| `dev/release-tools/tests/git.test.ts` | `git helpers > getShortSha :: returns a 7-character hash` | Vitest sync; real Git | `RELEASE-REQ-025` |
| `dev/release-tools/tests/git.test.ts` | `git helpers > getCommitsBetween :: returns commits between two refs` | Vitest sync; tag and two commits | `RELEASE-REQ-025` |
| `dev/release-tools/tests/git.test.ts` | `git helpers > getCommitsBetween :: returns all commits from HEAD when sinceRef is null` | Vitest sync | `RELEASE-REQ-025` |
| `dev/release-tools/tests/git.test.ts` | `git helpers > createTag :: creates a tag` | Vitest sync | `RELEASE-REQ-025` |
| `dev/release-tools/tests/git.test.ts` | `git helpers > createTag :: throws when tag exists and ignoreIfExists is false` | Vitest sync negative | `RELEASE-REQ-025` |
| `dev/release-tools/tests/git.test.ts` | `git helpers > createTag :: skips when tag exists and ignoreIfExists is true` | Vitest sync | `RELEASE-REQ-025` |
| `dev/release-tools/tests/git.test.ts` | `git helpers > pushTag :: pushes a tag to remote` | Vitest sync; local bare remote | `RELEASE-REQ-025` |
| `dev/release-tools/tests/git.test.ts` | `git helpers > pushTag :: pushes tag and branch when pushBranch is true` | Vitest sync; local bare remote | `RELEASE-REQ-025` |
| `dev/release-tools/tests/git.test.ts` | `git helpers > pushTag :: skips when tag exists on remote and ignoreIfExists is true` | Vitest sync; local bare remote | `RELEASE-REQ-025` |
| `dev/release-tools/tests/commands/create-release-branch.test.ts` | `create-release-branch :: creates branch with single iOS bump` | Vitest async declaration; real temporary Git and filesystem; patch | `RELEASE-REQ-029` |
| `dev/release-tools/tests/commands/create-release-branch.test.ts` | `create-release-branch :: creates branch with single Android bump` | Vitest async; minor | `RELEASE-REQ-029` |
| `dev/release-tools/tests/commands/create-release-branch.test.ts` | `create-release-branch :: creates branch with both iOS and Android bumps` | Vitest async; major and minor | `RELEASE-REQ-029` |
| `dev/release-tools/tests/commands/create-release-branch.test.ts` | `create-release-branch :: creates branch with node-sdk and browser-sdk bumps` | Vitest async; independent bases, minor and patch | `RELEASE-REQ-029` |
| `dev/release-tools/tests/commands/create-release-branch.test.ts` | `create-release-branch :: creates branch with --node flag` | Vitest async; bindings follow release version | `RELEASE-REQ-029` |
| `dev/release-tools/tests/commands/create-release-branch.test.ts` | `create-release-branch :: creates branch with --wasm flag` | Vitest async; bindings follow release version | `RELEASE-REQ-029` |
| `dev/release-tools/tests/commands/create-release-branch.test.ts` | `create-release-branch :: creates branch with all SDKs` | Vitest async; six selected SDKs | `RELEASE-REQ-029` |
| `dev/release-tools/tests/commands/create-release-branch.test.ts` | `create-release-branch :: throws error when no SDKs are bumped` | Vitest async declaration; negative | `RELEASE-REQ-029` |
| `dev/release-tools/tests/commands/create-release-branch.test.ts` | `create-release-branch :: includes previous_release_tag when matching git tag exists` | Vitest async; tagged baseline | `RELEASE-REQ-029` |

Runner requirements: Node 22 or later; `yarn test` runs `vitest run`. Filesystem cases need writable temporary directories. Git cases need the Git CLI and disable signing in throwaway repositories. Push cases use a local bare remote. Ten `.each` declarations expand to 47 runtime cases. No source declaration uses skip, todo, or only.
