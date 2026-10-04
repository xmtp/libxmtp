# Installed release benchmarks

This suite implements the Task 10 release measurements and the P26/V14 gate.
The checked-in baseline lock pins published Node 6.1.0, browser 7.1.0, Android
4.11.0, and Swift 4.11.0 packages. It records registry and release provenance. The resolver verifies each selected
archive against its published checksum before it writes the lock. Android source
provenance uses the exact release tag and its version file. Its publication
evidence is the AAR Last-Modified header from Maven, recorded as an artifact
upload date. No GitHub Android release date is available.
It is not a complete package inventory or a performance result.

**Release status: PENDING.** Run the suite again after the final integrated head
has produced all four installed release packages. Keep the package hash review
and the callback correctness/lifetime matrix with that result. A successful
performance report does not close either check.

## Measurement rules

- Run at least 20 complete old/new pairs on one runner. Alternate the order for
  each pair. Fixture setup and per-pair reset finish before timing starts.
- Use the published old package and the new integrated release package through
  their public APIs. Use the same backend, dataset, enrichment, compiler mode,
  runtime, hardware, cache policy, and production flags for each pair.
- Report p50 and p95. Resample old/new pair indices together 10,000 times. The
  interval is the percentile 95% interval for `median(new) / median(old)`.
- Fail latency, memory, or build time when the interval lower bound is above
  1.20. Fail throughput when its interval upper bound is below 0.80. Record
  smaller changes. Do not convert an uncertain interval into a failure.
- Fail complete raw or compressed package size when its direct ratio is above
  1.20. Include runtime dependencies, native libraries, worker code, and WASM.
  Compression uses a deterministic tar archive and gzip level 9 on both sides.
- On Swift and Kotlin, run a separate 20-pair class/record check. Alternate
  record/class order. Fail when the median class/record ratio is above 1.20.
  Read the same 1,000 rich messages, serialize outside timing, then perform
  10,000 conversions per run. This check does not enter page memory samples.
- Report correctness, deadlock, use-after-end, and retained growth separately.
  The host reports only what the timed operation establishes. Unknown correctness, deadlock, and lifetime
  results stay `null`; the independent callback matrix must resolve them.

The former private-binding Node microbenchmark remains an internal diagnostic.
Its 2x threshold is removed. The unused 5% mobile lift probes are replaced by
this class/record gate.

## Workload boundaries

`fixtures.py` creates 10,000 primary messages. Every four messages contain text,
a reply with its eager text parent, a 128-byte attachment, and text with a `+1`
reaction. Node, Swift, and Kotlin stream runs select all 10,000 messages and
2,500 reaction events. Browser stream runs select the first 500 messages and
125 reaction events. This is the version 2 Browser fixture, with seed
`xmtp-cutover-browser-stream-500-v2`. Both installed Browser packages use that
same fixture and its recorded digest. Page and other workloads keep their
previous size. Hosts normalize actual public values. The driver checks the
complete content digest, primary count, and Browser event count. It does not
accept a host-supplied digest for page or stream data.

| Workload             | Timed work                                                                                                                  |
| -------------------- | --------------------------------------------------------------------------------------------------------------------------- |
| `cold_start`         | A fresh `Client.create`, after module load and signer key generation. Browser worker creation occurs inside this operation. |
| `page`               | Read and normalize 1,000 rich messages in ascending order.                                                                  |
| `stream`             | Publish and consume 12,500 prepared events for Node, Swift, and Kotlin, or 625 for Browser. Normalize 10,000 or 500 rich primary messages, respectively. |
| `callback_immediate` | Last signer callback entry through completion of client creation. Includes signing and subsequent backend work.             |
| `callback_slow`      | The same callback boundary with a controlled 25 ms hold before signing.                                                     |
| `build_clean`        | The declared complete production build after deleting its declared output/cache directory.                                  |
| `build_warm`         | The same production build after one untimed priming build.                                                                  |

Stream reset creates fresh sender and receiver databases and pending messages
for each pair. Each host starts collection, then gives both packages the same
untimed one-second subscription grace. The old published streams have no
readiness API. A missing event causes failure or timeout. All four adapters retain and decode
actual live text, reply bodies, attachment bytes, and reaction bodies. A common
host accumulator joins reply parents and reactions from those delivered values
inside the timer. Both old and new packages perform that work. When an SDK also
supplies eager parent or reaction values, the accumulator preserves those values
and checks their content and duplicate counts against the live events. Eager
reaction completeness stays PENDING: the reader queries stored reactions when
it lifts each item. That database snapshot can differ from stream arrival order
and from the final collection. A missing eager reaction is not proved by this
workload. A separate deterministic eager snapshot check is required. Native legacy streams have no eager reaction children. The Android raw reply
has no eager parent. Both packages include the same host enrichment cost. Stream observations never come from a history query. The accumulator emits
primary values in fixture ID order. It does not prove stream arrival order.

Node, Swift, and browser memory is the largest sampled RSS sum of the host
process tree during the complete invocation. The driver samples every 10 ms.
This includes client opening and host tools; it is broader than the operation
timer. Node also reports its process maximum RSS. Browser scope includes Vite,
Chromium, worker, and renderer processes. Kotlin reports the Android app process
PSS sampled every 10 ms. Build memory uses the build process tree on every host.
These scopes must be recorded in the configuration. Do not compare RSS and PSS
across targets. Browser intersects each observed main-thread task with the exact
operation timer window. It reports only overlaps above 50 ms. Setup and teardown
are outside that window. The JSON report contains the window, counts and durations.

## Prepare installed inputs

1. Resolve the published baselines with
   `dev/nix-shell 'just sdk cutover-bench-baselines /absolute/path/baselines.json'`.
   Review version, commit, package checksum, and published date against the
   checked-in lock. Do not use a development checkout as the old package.
2. Materialize each complete installed dependency closure in a separate
   directory. Dereference symlinks. Keep package manifests, lock files, native
   binaries, workers, pure codecs, WASM, and runtime dependencies. Exclude
   development tools from both distributions by the same rule. Do not count
   only a JavaScript entry file or only a native archive. Inventory all files.
3. Build the new inputs from the final integrated source head with release
   flags. Record the source commit and artifact receipt. Keep both closures
   immutable during the run. The runner hashes all files before and after.
4. Prepare host tools outside those closures. Pin Node, viem, Vite, Playwright,
   Chromium, Xcode/Swift, JDK, Gradle, Android SDK, and device versions. Record
   them in the configuration. Use the same versions for each paired run.
5. Start the shared backend and record its source/image hash and endpoint in
   the runner metadata. Use one idle runner and one fixed browser origin or
   Android device. Do not run other builds during measurements.

## Host adapters

Every runner command calls `hosts/driver.py` with a side-specific JSON file.
That file contains `host_command`, `build_command`, and `build_cache`. Each
command is an argv array. The cache must be inside that side's runner state
folder. Build commands must use the production source and flags recorded for
that package. Record all build output/cache locations in the cleanup policy;
the runner deletes only the declared cache. Builds must not modify the frozen
installed closure.

For Node, `host_command` is `node hosts/node.mjs /absolute/path/node-host.json`.
The host file names `sdk_entry`, optional `pure_entry` for the public root,
`accounts_entry` for viem accounts, and `backend_url`. SDK entry files must
resolve inside the recorded installed closure. Use `node --conditions=production`
if the package has production export conditions. The runner sets
`NODE_ENV=production` for every host command.

For browser, use `node hosts/browser.mjs /absolute/path/browser-host.json`.
The host file has the Node fields plus `vite_entry`, `playwright_entry`,
`chromium_executable`, `browser_port`, `package_root`, and `tools_root`.
The entry files must match the installed manifest public ESM exports. The new
browser `pure_entry` must match `./pure`. Node codecs use the public root; Node
has no `./pure` export. Private in-closure overrides are rejected. Vite serves the installed release
code, and Chromium uses a persistent profile per side. Keep `browser_port`
fixed. COOP and COEP headers allow the real worker to run.

For Swift, use a Release UIKit app on one fixed arm64 iOS Simulator. Both
installed public products run on this target: old `XMTPiOS` and new `XmtpSdk`.
The app uses no `@testable` imports. A macOS executable cannot link the new
XCFramework. Prepare a JSON file with `side`, `package_root`, and `assets`
(the same public/native asset map used by the runner). Run once per side:

```sh
NIX_DEVSHELL=ios dev/nix-shell 'just sdk cutover-bench-ios-prepare /absolute/ios-prepare.json /absolute/old-host'
NIX_DEVSHELL=ios dev/nix-shell 'just sdk cutover-bench-ios-build /absolute/old-host SIMULATOR-UDID /absolute/old-derived'
```

The build receipt binds the app bytes, source identity, public product, package
closure, Xcode version, simulator, and resolved dependency bytes. For a package
with transitive Swift dependencies, set `dependency_root` to a directory inside
`package_root`. It must contain the resolved `Package.resolved`, `checkouts`,
and `artifacts` from a prior package resolution. Copy these files, omit `.git`,
and materialize symlinks before freezing the complete installed closure. The
build uses the frozen pins and requires the resolved source and binary bytes to
match this snapshot. Keep each side's resolution and derived data separate.
Start the existing `hosts/signer-server.mjs` with the installed viem accounts
module and a fixed port. Use the same server for both sides.

Use `python3 hosts/ios.py /absolute/ios-host.json` as `host_command`. This JSON
needs `build_receipt` (the generated `build-receipt.json`), `simulator_udid`,
`backend_url`, `signer_url`, and `timeout_seconds`. Boot this explicit simulator
before running. The adapter timeout bounds its operation. If the outer runner
timeout kills the launcher, the runner independently terminates the registered
app and saves `runner-cleanup.json`. This cleanup has a separate 15-second
limit. A cleanup failure fails the run. Loopback URLs reach the Mac from the
simulator. Physical devices need a separate bridge.
The adapter verifies the app bytes, stages the original request in an envelope,
starts a fresh app process, validates the response, and terminates the app.
The app keeps its page state under its Application Support directory.

Swift workload `peak_memory_bytes` is the app's positive resident high-water
mark from process start through the operation. It includes startup, fixture,
Swift runtime, and native SDK memory. A failed memory query fails the sample.
The driver never replaces this value with Mac launcher RSS. Build workloads
retain the declared command's wall time and Mac process-tree memory. Declare
whether the build command measures SDK production, app compilation, or both;
app compilation alone does not prove the SDK/CI build-cost gate. SDK duration
fields exclude app installation, launch, file transport, and response writing.

Run `just sdk cutover-bench-ios-controls <host-config> <output>` in the iOS
Nix shell with the signer running. This command runs real bridge controls.
Bridge probes use `phase: "probe"` and are excluded from benchmark samples.
They can test `probe: "signer"`, `probe: "error"`, `allocate_bytes`, and separate
`transport_delay_ms`/`operation_delay_ms` values. Save their request and response
artifacts. Run a large touched allocation followed by a fresh empty process to
prove the memory source. Probe results do not replace public SDK workload,
callback lifetime, or the 20 paired performance runs.

For Android, `hosts/android` builds a separate release APK per side. Its main
instrumentation code uses the installed public SDK. The package closure must
contain all transitive AAR/JAR files, including coroutines and protocol jars.
Do not add both a JNA AAR and a duplicate JNA JAR. Build with:

```sh
NIX_DEVSHELL=android dev/nix-shell 'sdks/android/gradlew -p crates/xmtp_sdk/benchmarks/hosts/android assembleRelease -PbenchmarkSide=new -PsdkPackageRoot=/absolute/new-closure -PbenchmarkBuildDirectory=/absolute/new-state/android-build --no-daemon'
```

Use `python3 hosts/android.py /absolute/path/android-host.json` as the host
command. The file needs `adb`, `device_serial`, `apk`, `backend_url`, and
`signer_url`. Start `node hosts/signer-server.mjs /absolute/viem/accounts.js PORT`
on the host. Forward its loopback port with `adb reverse tcp:PORT tcp:PORT`.
Forward the backend port too when needed. The signer serves generated test
accounts only. The launcher installs the release APK on setup, pushes the
fixture through the app's external files directory, and reads a fresh result.
Use a dedicated test device with free storage. The package ID is
`org.xmtp.benchmark.old` or `org.xmtp.benchmark.new`.

## Runner configuration and execution

Create one JSON configuration per target. All paths must be absolute. Required
fields are:

- `schema: 1`, `target`, `purpose: "release"`, `pairs: 20` or more, and a finite
  `timeout_seconds` that covers fixture setup and stream publication.
- `runner_class`, `os`, `hardware`, `runtime`, `clean_build_policy`,
  `warm_build_policy`, `memory_scope`, and `cache_policy`. Include the exact
  backend version and tool versions in these metadata strings.
- `enrichment: ["decoded_content", "reply_parent", "reactions", "attachments"]`.
- `old` and `new`: `root`, `version`, `commit`, `compiler`, `production_flags`,
  `provenance`, `profile: "release"`, `public_api: true`, `command`,
  `adapter_sources`, and `assets`. Set `old.published: true` and
  `new.integrated_head: true` only after checking the inputs.

`command` calls Python and the driver with its side's JSON path.
`adapter_sources` lists all adapter, configuration, build scaffold, and helper
files. The runner hashes their contents before and after. `assets` maps each
required role to nonempty arrays of paths relative to the installed root:
Swift/Kotlin use `public,native`; Node uses `public,native,runtime`; browser
uses `public,worker,wasm,pure,runtime`. The inventory includes every file under
`root`, not just those named as role examples.

```sh
dev/nix-shell 'just sdk cutover-bench node /absolute/node.json /absolute/results-node'
python3 crates/xmtp_sdk/benchmarks/runner.py analyze /absolute/results-node/ledger.json /absolute/recomputed.json
```

The output directory must be new. It contains every request response, stderr,
raw pair, source hash, package inventory, report, and any failure. Host state
holds actual normalized observations. A timeout kills the host process group.
Android instrumentation may need an explicit `adb shell am force-stop` after
host interruption. Never continue that sample or silently reuse its result.
Exit 0 means the measured performance checks passed, 1 means a performance or
safety check failed, and 2 means an incomplete/invalid run. The release status
remains PENDING in every case until the separate reviews are complete.

## Harness checks

```sh
dev/nix-shell 'just sdk cutover-bench-check'
python3 crates/xmtp_sdk/benchmarks/prove_gates.py /absolute/mutation-proofs
python3 crates/xmtp_sdk/benchmarks/control.py /absolute/four-target-controls
python3 crates/xmtp_sdk/benchmarks/prove_repairs.py /absolute/repair-controls
node crates/xmtp_sdk/benchmarks/entry_controls.mjs
node crates/xmtp_sdk/benchmarks/reaction_controls.mjs
dev/nix-shell 'just sdk cutover-bench-stream-check /absolute/stream-controls /absolute/browser-node_modules'
```

The tests cover interval direction, strict boundaries, uncertain intervals,
pair preservation, size growth, unsafe outcomes, invalid observations,
incomplete samples, and the separate mobile check. `prove_gates.py` weakens
each implementation in a temporary copy and requires an assertion failure,
then reruns the restored tests. `control.py` runs all four subprocess paths
with synthetic equal values. It saves full ledgers and keeps the release gate
pending. Those controls are not evidence of real SDK speed or package size.

The stream controls execute the actual JavaScript live decoder and measured
helper in Node and Chromium. The Swift and Kotlin controls compile the same
live enrichment functions used by their host adapters. Each control retains a
correct 10,000-message history result as a decoder stress check, including the
Browser control, while dropping or changing live text,
reply bodies, attachment bytes, or reaction content. Bad live values must fail;
restored values must pass. Native controls also change eager parent content.
All four controls reject a weaker history-fallback implementation. To reproduce
an earlier committed JavaScript helper exactly, run `stream_controls.py` with
`--before-commit <commit>`. Logs and command receipts identify each failure and
restored pass. These are boundary controls; final installed SDK runtime checks
remain separate release gates.

## Known limitations

The owner accepts the security risks of this benchmark harness. Benchmark
signing keys are disposable. The harness can retain these keys in benchmark
state and send them to the configured signer service. Key storage and signer
endpoint hardening are outside the cutover scope. The resolver trusts configured
registry and release URLs and their redirects. These are accepted harness risks.

Correctness, measured safety results, observed order, artifact checksums,
workloads and the 20% thresholds remain required.

Owner decision: [Phase 1 plan](https://plan.ref.tools/vRG5sTDlgoKQ911m) and
[finish plan](https://plan.ref.tools/TiFDtuzx3U19olnv).
