# Installed release benchmarks

This suite implements the Task 10 release measurements and the P26/V14 gate.
The checked-in baseline lock pins published Node 6.1.0, browser 7.1.0, Android
4.11.0, and Swift 4.11.0 packages. It records registry and release provenance.
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
  The host reports only what the timed operation establishes. Unknown lifetime
  results stay `null`; the independent callback matrix must resolve them.

The former private-binding Node microbenchmark remains an internal diagnostic.
Its 2x threshold is removed. The unused 5% mobile lift probes are replaced by
this class/record gate.

## Workload boundaries

`fixtures.py` creates 10,000 primary messages. Every four messages contain text,
a reply with its eager text parent, a 128-byte attachment, and text with a `+1`
reaction. There are 2,500 additional reaction events. Hosts normalize actual
public values. The driver checks the complete content digest and message count.
It does not accept a host-supplied digest for page or stream data.

| Workload             | Timed work                                                                                                                  |
| -------------------- | --------------------------------------------------------------------------------------------------------------------------- |
| `cold_start`         | A fresh `Client.create`, after module load and signer key generation. Browser worker creation occurs inside this operation. |
| `page`               | Read and normalize 1,000 rich messages in ascending order.                                                                  |
| `stream`             | Publish 12,500 prepared events, consume all event IDs, then read and normalize the same 10,000 rich primary messages.       |
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
supplies eager parent or reaction values, the accumulator checks those values
against the live events. Native legacy streams therefore include the required
host enrichment cost. Stream observations never come from a history query.

Node, Swift, and browser memory is the largest sampled RSS sum of the host
process tree during the complete invocation. The driver samples every 10 ms.
This includes client opening and host tools; it is broader than the operation
timer. Node also reports its process maximum RSS. Browser scope includes Vite,
Chromium, worker, and renderer processes. Kotlin reports the Android app process
PSS sampled every 10 ms. Build memory uses the build process tree on every host.
These scopes must be recorded in the configuration. Do not compare RSS and PSS
across targets. Browser records every observed main-thread task above 50 ms
while the measurement runs; the JSON report contains counts and durations.

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
The host file names `sdk_entry`, `pure_entry` for the new `./pure` export,
`accounts_entry` for viem accounts, and `backend_url`. SDK entry files must
resolve inside the recorded installed closure. Use `node --conditions=production`
if the package has production export conditions. The runner sets
`NODE_ENV=production` for every host command.

For browser, use `node hosts/browser.mjs /absolute/path/browser-host.json`.
The host file has the Node fields plus `vite_entry`, `playwright_entry`,
`chromium_executable`, `browser_port`, `package_root`, and `tools_root`.
The entry files must be the installed public exports. The new `pure_entry`
must resolve to its public WASM codec export. Vite serves the installed release
code, and Chromium uses a persistent profile per side. Keep `browser_port`
fixed. COOP and COEP headers allow the real worker to run.

For Swift, copy `SwiftPackage.swift` to an isolated host directory as
`Package.swift`, with `SwiftSupport.swift`, `SwiftLive.swift`, `SwiftOld.swift`, and `SwiftNew.swift`.
Set `BENCHMARK_SIDE=old|new` and `BENCHMARK_SDK_PACKAGE` to the local installed
SwiftPM product. Resolve its complete pinned dependencies before timing.
Build with `NIX_DEVSHELL=ios dev/nix-shell 'swift build -c release ...'`.
The executable accepts a host JSON file with `backend_url` and
`signer_command`, such as `["/absolute/node", "/absolute/hosts/signer.mjs",
"/absolute/viem/_esm/accounts/index.js"]`. Use the same signer helper on both
sides. The new module name is `XmtpSdk`; the old module name is `XMTPiOS`.
The host uses no `@testable` imports.

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
correct 10,000-message history result while dropping or changing live text,
reply bodies, attachment bytes, or reaction content. Bad live values must fail;
restored values must pass. Native controls also change eager parent content.
All four controls reject a weaker history-fallback implementation. To reproduce
an earlier committed JavaScript helper exactly, run `stream_controls.py` with
`--before-commit <commit>`. Logs and command receipts identify each failure and
restored pass. These are boundary controls; final installed SDK runtime checks
remain separate release gates.
