# Nix cache runner spike

## Scope

This branch experiments with the `dev` and `release` members built by the two
`build` jobs in `post-merge.yml`. The production workflow is unchanged. The
separate advisory React Native cache/device jobs are outside this experiment.

The spike uses standard `ubuntu-24.04` and Apple Silicon `macos-15` runners.
It reads the trusted `xmtp` cache and cache.nixos.org, receives no cache writer
credentials, and neither uploads nor pins Nix outputs.

## Grouping

The warm sets have 47 entries and 44 distinct derivations after removing aliases.
The branch groups them into 16 build jobs: 10 Linux and 6 macOS. At most 10 matrix
jobs run at once. An inventory job and a results job bring the workflow total to
18 jobs. Inventory generation rejects missing, extra, or multiply assigned
member names.

| Linux | macOS |
| --- | --- |
| Core shells | Core shells |
| JS shells | JS shells |
| Android shells | Android shell and fast SDK |
| React Native JS | JS SDKs |
| JS SDKs and fixtures | Services and backend image |
| Android SDK | Apple fast/release frameworks and Darwin Node SDK |
| Android example | |
| Backend and image | |
| Node GNU, x64 and arm64 | |
| Node musl, x64 and arm64 | |

The fast Apple framework uses the host and matching simulator slices. Release
uses those same slices plus the device slice. Their packaging derivations differ,
but grouping them shares the expensive native compilation. Both outputs remain
requested so both can be cached in a future production implementation.

## Method

Every group gets a fresh runner and one `nix build --keep-going` invocation. A
run-specific comment in `xmtp_common` changes project source derivations, so the
trial measures source builds rather than only downloading unchanged products.
Dependency layers can still substitute. Shell and tool-only outputs can remain
unchanged; each artifact records which requested derivations changed.

Each job retains its initial hardware/disk snapshot, build output, and JSON
measurements for 14 days. Disk and Linux available RAM are sampled every five
seconds during the build command; these are approximate minima. macOS reports
memory pressure and swap snapshots. The elapsed measurement includes target
evaluation and realization, but excludes runner setup. The job timeout is the
production cache job's 90-minute budget.

These are standard runner image measurements, not a test under an enforced
14 GB disk quota. The existing Nix setup removes unused preinstalled toolchains
on Linux. Actual free disk on both platforms exceeded GitHub's documented
minimum. Cached dependencies and the forced source change also mean this is
neither a fully cold dependency build nor a controlled comparison with a normal
trunk push. Cache upload and pin time are excluded.

Runner limits and image contents:

- [GitHub-hosted runner specifications](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)
- [macOS 15 Apple Silicon image, including Xcode 26.3](https://github.com/actions/runner-images/blob/main/images/macos/macos-15-arm64-Readme.md)

## Measurements

The measured [workflow run](https://github.com/xmtp/libxmtp/actions/runs/39990187165)
uses commit `5532dd246a`, which had 17 build groups. The branch then combined
`apple-fast` and `apple-release` into `apple` without restarting the experiment.
The combined Apple group has not been timed as one job; the two original rows
below are separate measurements. All other group boundaries are unchanged.

All 17 measured groups passed, covering all 44 distinct warm members. 26
requested derivations changed after the source marker; the other 18 were shell
or tool inputs that could substitute. The complete run took **41m25s**,
including the **5m17s** inventory job, queueing under the 10-job cap, setup, and
results collection. Total runner time was 140.3 Linux minutes and 69.6 macOS
minutes, all on standard GitHub-hosted runners.

The Android example was the longest job at **32m32s**, including **30m55s** of
measured evaluation/build time. The next longest was Linux Android SDK at
**24m55s**. The slowest macOS job was Android cross-compilation at **15m29s**;
Apple release took **14m55s**.

### Linux

| Group | Full job | Evaluation/build | Min free disk (GiB) | Min available RAM (GiB) |
| --- | ---: | ---: | ---: | ---: |
| android-example | 32m32s | 30m55s | 64.7 | 6.0 |
| android-sdk | 24m55s | 23m51s | 69.2 | 7.4 |
| android-shells | 3m09s | 2m02s | 84.3 | 12.9 |
| backend | 6m25s | 4m16s | 93.3 | 12.7 |
| core-shells | 2m38s | 1m24s | 92.8 | 13.0 |
| js-sdks-and-fixtures | 20m04s | 18m18s | 85.4 | 8.7 |
| js-shells | 2m43s | 0m45s | 98.1 | 13.5 |
| node-gnu | 18m34s | 17m14s | 86.9 | 10.7 |
| node-musl | 9m31s | 7m45s | 88.8 | 8.2 |
| react-native-js | 14m14s | 12m35s | 85.3 | 10.7 |

### macOS

| Group | Full job | Evaluation/build | Min free disk (GiB) | Min available RAM (GiB) |
| --- | ---: | ---: | ---: | ---: |
| android | 15m29s | 14m11s | 15.5 | Not sampled |
| apple-fast | 13m02s | 11m45s | 29.9 | Not sampled |
| apple-release | 14m55s | 13m44s | 27.6 | Not sampled |
| core-shells | 3m41s | 2m33s | 33.9 | Not sampled |
| js-sdks | 11m50s | 10m34s | 29.7 | Not sampled |
| js-shells | 2m44s | 1m36s | 38.8 | Not sampled |
| services | 7m54s | 6m46s | 29.8 | Not sampled |

## Resource findings

The artifacts confirm 4 CPUs and about 15.6 GiB usable RAM on Linux, and 3 CPUs
and 7 GiB RAM on macOS. All requested builds completed within the 90-minute job
budget. The lowest sampled Linux available RAM was 6.0 GiB, in the Android
example job. That job's free swap decreased from 3.0 to about 1.5 GiB; the spike
changed no swap settings. macOS artifacts contain memory-pressure and swap
snapshots rather than a minimum available-RAM measurement.

The lowest sampled free disk was 64.7 GiB on Linux and 15.5 GiB on macOS. However,
several jobs used more than 14 GB of additional disk **after Nix setup**:

| Job | Approximate peak additional disk (GiB) |
| --- | ---: |
| Linux Android example | 38.8 |
| Linux Android SDK | 34.3 |
| macOS Android | 26.8 |
| Linux Android shells | 19.2 |
| Apple release | 14.7 |

These figures subtract each job's sampled minimum free space from its pre-build
free space. They include downloads and temporary build files, but omit Nix setup
itself. The builds fit the actual standard runner images, with the existing
Linux disk cleanup. This does **not** demonstrate that all builds fit with only
14 GB free, or that a fully cold dependency/toolchain rebuild fits.

## Production reference

The [earlier source-changing production run](https://github.com/xmtp/libxmtp/actions/runs/39968639052)
spent 20m26s on the Linux cache job and 22m11s on macOS. The
[next production run](https://github.com/xmtp/libxmtp/actions/runs/39985092898)
spent 3m09s and 5m53s, respectively, with substantially more cache reuse.
Those jobs include cache uploads and pins. Their source changes and dependency
cache state differ from this spike, so these are reference times rather than
an apples-to-apples speed comparison.

## Verdict

The complete requested output set built successfully on current standard
GitHub-hosted public runners, with cached dependency layers and the existing
Linux cleanup. The branch reduces the matrix to 16 logical groups, capped at
10 concurrent builds. No production workflow or cache-writing policy changes.
The remaining measurement gap is the combined Apple group's own elapsed time;
its fast and release outputs passed in separate jobs with shared compilation
repeated between them.

Local validation: workflow syntax, Python lint/format, `just lint-config`, matrix
coverage of all 44 unique members, and measurement success/failure smoke checks
passed. All 227 Python dev tests passed serially. Parallel `just test dev` runs
intermittently hit the existing emulator fixture's short ADB timeout; its focused
run and the serial suite passed. Markdown lint passed for this report.
