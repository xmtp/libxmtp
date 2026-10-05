# SDK host benchmarks

These runners measure the staged SDK package through its public API on one
host: Node, browser (Chromium), Swift (iOS Simulator) or Kotlin (Android). A
run records absolute numbers with p50 and p95. It has no pass or fail line and
no comparison. CI does not run it.

The suite measures only the binding and host-runtime layer. The Rust criterion
benches in `crates/xmtp_mls/benches` and `crates/xmtp_db/benches` measure the
core.

## Measurements

| Workload     | Timed work                                                                                      |
| ------------ | ----------------------------------------------------------------------------------------------- |
| `cold_start` | `Client.create` with a fresh database and a real ECDSA signer, after module load.               |
| `page`       | Read 1,000 rich messages (text, reply with parent, attachment, reaction) and normalize them.    |
| `stream`     | Publish 1,000 messages and 250 reactions while one group stream reads all 1,250 events.         |

Each workload reports `duration_ms` and `peak_memory_bytes`. Stream also
reports `messages_per_second`. The browser reports long tasks above 50 ms
inside the timer (`long_tasks` count and `long_task_ms` total). The run also
records the package size, raw and as a deterministic tar with gzip level 9.

Memory scope differs per host. Do not compare it across hosts:

- Node and browser: the peak RSS of the host process tree, sampled every 10 ms.
  The browser tree includes Vite and Chromium.
- Swift: the app's resident high-water mark.
- Kotlin: the app's PSS, sampled every 10 ms.

The runner checks every sample: the page must equal the fixture, in order, and
the stream must deliver each expected event once. Stream results are counts,
not content. The run fails, without `results.json`, when the package or a
runner source changes during it. The results name the measured package path
and hash. They are measurements, not an attested comparison.

## Run

Start this worktree's backend with `dev/nix-shell 'just backend up'`. Stage the
package for the host, then run the benchmark. The recipe runs in the Nix shell
that `NIX_DEVSHELL` names. Set it inside the command, because the `js` and
`android` shells have no `just`.

- `node`: stage with `just sdk generate node` and `just sdk stage node`. Run
  `dev/nix-shell 'just sdk bench node'`.
- `browser`: stage with `just sdk generate browser` and
  `just sdk stage browser`. Run
  `dev/nix-shell 'NIX_DEVSHELL=js just sdk bench browser'`.
- `swift`: stage with `just sdk generate swift`, `just sdk mobile-build ios`
  and `just sdk mobile-stage ios`. Boot one iOS Simulator. Run
  `dev/nix-shell 'NIX_DEVSHELL=ios just sdk bench swift'`.
- `kotlin`: stage with `just sdk generate kotlin`,
  `just sdk mobile-build android` and `just sdk mobile-stage android`. Start
  one device or emulator. Run
  `dev/nix-shell 'NIX_DEVSHELL=android just sdk bench kotlin'`.

Options: `--samples N` (default 5), `--output DIR` (a new directory; the
default is `target/sdk-bench/<host>-<time>`), `--timeout SECONDS` per host
call, `--simulator UDID`, `--device SERIAL`, and `--keep-state`.
`XMTP_SDK_PACKAGES_DIR` selects another staged package directory.

The output directory holds `results.json` (summary, raw samples, package size,
commit and environment) and `logs/` (stderr and launcher logs per call). Swift
and Kotlin runs build the host app into the output directory and start
`hosts/signer-server.mjs`, which signs with generated test accounts.

`dev/nix-shell 'just sdk bench-check'` runs `test_bench.py`,
`test_workload.mjs` and the static runner checks (`bench.py check`). It needs
no backend, device or SDK build.

## Files

- `bench.py` runs setup, then for each workload and sample a reset call and a
  measure call. Every call is a fresh host process or app launch.
- `fixtures.py` is the message dataset. `packages.py` measures package size.
- `hosts/processes.py` runs a host and samples its process-tree RSS.
- `hosts/node.mjs`, `hosts/browser.mjs`, `hosts/browser-page.mjs`,
  `hosts/browser.html`, `hosts/sdk.mjs` and `hosts/workload.mjs` are the Node
  and Chromium runners.
- `hosts/ios_host.py`, `hosts/ios/`, `hosts/SwiftSupport.swift` and
  `hosts/SwiftSdk.swift` are the Release iOS Simulator app and its launcher.
  The launcher stops the app after each call, also after a timeout.
- `hosts/android_host.py` and `hosts/android/` are the release APK and its
  instrumentation launcher.
- `test_bench.py` checks the memory sampler, the percentile helper, the sample
  checks, the run integrity check and the iOS timeout cleanup.
  `test_workload.mjs` checks that the stream teardown runs once, outside the
  timer, and also after a read or publish failure.
