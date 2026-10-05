# SDK host benchmarks

These files measure the generated SDK through its installed public packages on
each host: Swift, Kotlin, Node, and browser. Each run records absolute numbers
(p50 and p95) for one package. There is no comparison and no pass or fail line.

The old-versus-new cutover harness is gone: no published baselines, paired
statistics, size or latency gates, parity enrichment, build receipts, or tool
controls. This directory is not a complete suite yet. No `just` recipe runs it.
A later change adds `just sdk bench` and `just sdk bench-check`.

## Files

- `runner.py` runs setup, then `samples` reset and measure calls for each
  workload, through one host command. It writes `ledger.json`, `report.json`,
  and `report.md` to a new output directory. Before it writes the report, it
  hashes the package and adapter sources again and fails the run if they
  changed.
- `fixtures.py` holds the deterministic message dataset. `packages.py`
  inventories the installed package closure and records its raw and
  compressed size.
- `hosts/driver.py` wraps a host executable. It adds process-tree memory
  (`hosts/processes.py`) and runs build workloads. Only the page workload
  returns observed values. Stream returns delivered-ID counts, which the
  runner checks against the fixture. Other workloads return completion.
- `hosts/node.mjs`, `hosts/browser.mjs`, `hosts/browser-page.mjs`,
  `hosts/browser.html`, `hosts/sdk.mjs`, and `hosts/workload.mjs` are the Node
  and Chromium hosts. The browser host serves the installed package with Vite
  and runs it in Playwright Chromium.
- `hosts/ios.py`, `hosts/ios/prepare.py`, `hosts/ios/BenchmarkApp.swift`,
  `hosts/ios/Info.plist`, `hosts/SwiftSupport.swift`, and
  `hosts/SwiftNew.swift` are the Release UIKit simulator host. The app reports
  its resident high-water memory. `ios_cleanup.py` lets the runner terminate
  the Simulator app after its outer timeout kills the launcher.
- `hosts/android.py` and `hosts/android/` are the release APK host. The
  instrumentation reports process PSS.
- `hosts/signer-server.mjs` signs with generated test accounts for the mobile
  hosts.
- `test_bench.py` checks the process-tree memory sampler, the outer-timeout
  app cleanup, the stream result shape, and that a run fails when the
  package or an adapter source changes during measurements. Run it with
  `python3 -m unittest discover -s crates/xmtp_sdk/benchmarks -p 'test_*.py'`.
