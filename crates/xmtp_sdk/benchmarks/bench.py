#!/usr/bin/env python3
"""Measure the staged SDK package on one host and write results.json.

Each workload sample is a fresh host process: one reset call, then one measure
call. The report gives p50 and p95 for each metric. It has no pass or fail line.
"""

import argparse
import contextlib
import datetime
import hashlib
import json
import math
import os
import platform
import shutil
import socket
import subprocess
import sys
import time
from pathlib import Path
from urllib.parse import urlsplit

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
sys.path.insert(0, str(HERE / "hosts"))

from fixtures import ROWS, canonical, dataset, digest, stream_events  # noqa: E402
from packages import measure_package  # noqa: E402
from processes import execute  # noqa: E402

import android_host as android  # noqa: E402
import ios_host as ios  # noqa: E402

HOSTS = ("node", "browser", "swift", "kotlin")
WORKLOADS = ("cold_start", "page", "stream")
MEMORY_SCOPE = {
    "node": "process-tree RSS",
    "browser": "process-tree RSS of Node, Vite and Chromium",
    "swift": "iOS app resident high-water",
    "kotlin": "Android app PSS, sampled every 10 ms",
}
VIEM = "sdks/{}/node_modules/viem/_esm/accounts/index.js"


class BenchError(Exception):
    pass


def percentile(values, fraction):
    ordered = sorted(values)
    position = (len(ordered) - 1) * fraction
    low, high = math.floor(position), math.ceil(position)
    return ordered[low] + (ordered[high] - ordered[low]) * (position - low)


def required(path, hint):
    path = Path(path)
    if not path.exists():
        raise BenchError(f"Missing {path}. {hint}")
    return path.resolve()


def staged(name, hint):
    base = Path(os.environ.get("XMTP_SDK_PACKAGES_DIR", ROOT / "target/sdk-packages"))
    return required(base / name, f"Run `{hint}` first.")


def tool(relative):
    return str(required(ROOT / relative, "Run `just install-js` first."))


def backend_url():
    url = os.environ.get("XMTP_BACKEND_URL")
    if not url:
        raise BenchError("XMTP_BACKEND_URL is not set. Run through `just sdk bench`.")
    return url


def free_port():
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        return probe.getsockname()[1]


def write_json(path, value):
    Path(path).write_text(json.dumps(value, indent=2) + "\n")
    return str(path)


@contextlib.contextmanager
def signer_server(log):
    """Sign with generated test accounts over HTTP for the mobile apps."""
    port = free_port()
    with open(log, "w") as output:
        server = subprocess.Popen(
            [
                "node",
                str(HERE / "hosts/signer-server.mjs"),
                tool(VIEM.format("node")),
                str(port),
            ],
            stdout=output,
            stderr=output,
        )
        try:
            for _ in range(100):
                try:
                    socket.create_connection(("127.0.0.1", port), 0.1).close()
                    break
                except OSError:
                    time.sleep(0.1)
            else:
                raise BenchError(f"Signer server did not start. See {log}")
            yield port
        finally:
            server.terminate()
            server.wait()


def js_host(name, config, out, timeout):
    """A Node process per call. Memory is the sampled process tree."""
    argv = [
        "node",
        str(HERE / f"hosts/{name}.mjs"),
        write_json(out / "host.json", config),
    ]

    def call(request, log):
        code, stdout, stderr, _, memory = execute(argv, json.dumps(request), timeout)
        log.with_suffix(".stderr").write_text(stderr)
        if code:
            raise BenchError(f"Host exited {code}. See {log.with_suffix('.stderr')}")
        response = json.loads(stdout)
        response["peak_memory_bytes"] = max(
            memory, response.get("peak_memory_bytes", 0)
        )
        return response

    return call


def open_host(host, args, out, stack):
    """Return (call, package path) for one host, after any app build."""
    if host == "node":
        package = staged("node", "just sdk stage node")
        config = {
            "sdk_entry": str(package / "entry.js"),
            "accounts_entry": tool(VIEM.format("node")),
            "backend_url": backend_url(),
        }
        return js_host("node", config, out, args.timeout), package
    if host == "browser":
        package = staged("browser", "just sdk stage browser")
        config = {
            "sdk_entry": str(package / "entry.js"),
            "pure_entry": str(package / "pure.js"),
            "accounts_entry": tool(VIEM.format("browser")),
            "vite_entry": tool("sdks/browser/node_modules/vite/dist/node/index.js"),
            "playwright_entry": tool("sdks/browser/node_modules/playwright/index.mjs"),
            "browser_port": free_port(),
            "package_root": str(package),
            "tools_root": str(ROOT / "node_modules"),
            "backend_url": backend_url(),
        }
        return js_host("browser", config, out, args.timeout), package
    if host == "swift":
        package = staged("ios", "just sdk mobile-stage ios")
        required(package / "Package.swift", "Run `just sdk mobile-stage ios` first.")
        udid = args.simulator or ios.booted_simulator()
        app = ios.build_app(package, out / "ios-app", udid)
        port = stack.enter_context(signer_server(out / "signer.log"))
        config = {
            "app_path": str(app),
            "simulator_udid": udid,
            "backend_url": backend_url(),
            "signer_url": f"http://127.0.0.1:{port}",
            "timeout_seconds": args.timeout,
        }
        ios.install(config)
        return lambda request, log: ios.invoke(config, request, log), package
    package = staged("android/xmtp-sdk.aar", "just sdk mobile-stage android")
    serial = args.device or android.only_device()
    apk = android.build_apk(package, out / "android-build")
    port = stack.enter_context(signer_server(out / "signer.log"))
    backend = backend_url()
    config = {"serial": serial, "apk": str(apk), "timeout_seconds": args.timeout}
    stack.enter_context(android.reverse(config, [port, urlsplit(backend).port]))
    config["host"] = {"backend_url": backend, "signer_url": f"http://127.0.0.1:{port}"}
    android.install(config)
    return lambda request, log: android.invoke(config, request, log), package


def check_measurement(response, workload, fixture, host):
    """Reject a sample whose observed values or counts are wrong."""
    if not (
        isinstance(response.get("duration_ms"), (int, float))
        and response["duration_ms"] > 0
    ):
        raise BenchError(f"{workload}: invalid duration_ms")
    if not response.get("peak_memory_bytes", 0) > 0:
        raise BenchError(f"{workload}: memory was not sampled")
    if workload == "page":
        observed = response.pop("observed_messages", None)
        if digest(observed) != digest(fixture["messages"]):
            raise BenchError("page: observed messages differ from the fixture")
    if workload == "stream" and response.get("streamed_events") != stream_events(
        fixture
    ):
        raise BenchError("stream: wrong number of streamed events")
    if host == "browser" and not isinstance(response.get("long_tasks_ms"), list):
        raise BenchError(f"{workload}: browser long tasks are absent")


def sources_digest():
    """Hash the runner and host sources. Ignored build output is excluded."""
    listed = subprocess.run(
        ["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard"],
        cwd=HERE,
        capture_output=True,
        check=True,
    ).stdout.split(b"\0")
    content = hashlib.sha256()
    for name in sorted(n for n in listed if n and b"__pycache__/" not in n):
        path = HERE / os.fsdecode(name)
        content.update(name + b"\0")
        content.update(
            hashlib.sha256(path.read_bytes()).digest() if path.is_file() else b"-"
        )
    return content.hexdigest()


def ready(response, phase):
    if response.get("ready") is not True:
        raise BenchError(f"Host {phase} is not ready")


def summarize(host, samples):
    rows = []
    for workload in WORKLOADS:
        values = [s for s in samples if s["workload"] == workload]
        metrics = {
            "duration_ms": [s["duration_ms"] for s in values],
            "peak_memory_bytes": [s["peak_memory_bytes"] for s in values],
        }
        if workload == "stream":
            metrics["messages_per_second"] = [
                s["streamed_events"] * 1000 / s["duration_ms"] for s in values
            ]
        if host == "browser":
            metrics["long_tasks"] = [len(s["long_tasks_ms"]) for s in values]
            metrics["long_task_ms"] = [sum(s["long_tasks_ms"]) for s in values]
        for metric, series in metrics.items():
            rows.append(
                {
                    "workload": workload,
                    "metric": metric,
                    "p50": percentile(series, 0.5),
                    "p95": percentile(series, 0.95),
                }
            )
    return rows


def run(host, args):
    out = Path(
        args.output
        or ROOT / "target/sdk-bench" / f"{host}-{time.strftime('%Y%m%d-%H%M%S')}"
    )
    out = out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    (out / "state").mkdir()
    (out / "logs").mkdir()
    fixture = dataset()
    (out / "state/fixture.json").write_bytes(canonical(fixture))
    samples = []
    sources = sources_digest()
    with contextlib.ExitStack() as stack:
        call, package = open_host(host, args, out, stack)
        measured = measure_package(package)
        base = {"state_directory": str(out / "state")}
        ready(call({**base, "phase": "setup"}, out / "logs/setup"), "setup")
        for workload in WORKLOADS:
            for sample in range(args.samples):
                request = {**base, "workload": workload, "sample": sample}
                log = out / f"logs/{workload}-{sample:03d}"
                reset = log.with_name(log.name + "-reset")
                ready(call({**request, "phase": "reset"}, reset), "reset")
                response = call({**request, "phase": "measure"}, log)
                check_measurement(response, workload, fixture, host)
                samples.append({"workload": workload, "sample": sample, **response})
                print(
                    f"{host} {workload} {sample}: {response['duration_ms']:.1f} ms",
                    file=sys.stderr,
                )
    # The results name one package and one set of runners. A change during the
    # run makes them wrong, so the run fails without results.json.
    if measure_package(package) != measured:
        raise BenchError("The package changed during the run")
    if sources_digest() != sources:
        raise BenchError("A runner or host source changed during the run")
    results = {
        "schema": 1,
        "host": host,
        "created": datetime.datetime.now(datetime.timezone.utc).isoformat(
            timespec="seconds"
        ),
        "commit": subprocess.run(
            ["git", "rev-parse", "HEAD"], cwd=ROOT, capture_output=True, text=True
        ).stdout.strip(),
        "environment": {"platform": platform.platform(), "machine": platform.machine()},
        "memory_scope": MEMORY_SCOPE[host],
        "fixture_sha256": digest(fixture),
        "package": measured,
        "summary": summarize(host, samples),
        "samples": samples,
    }
    write_json(out / "results.json", results)
    print(f"{'workload':<12} {'metric':<20} {'p50':>14} {'p95':>14}")
    for row in results["summary"]:
        print(
            f"{row['workload']:<12} {row['metric']:<20} {row['p50']:>14.2f} {row['p95']:>14.2f}"
        )
    for metric in ("raw_bytes", "compressed_bytes"):
        print(f"{'package':<12} {metric:<20} {results['package'][metric]:>14}")
    print(f"Results: {out / 'results.json'}")
    if not args.keep_state:
        shutil.rmtree(out / "state", ignore_errors=True)


def check():
    """Static checks of the runners: no backend, device or SDK build."""
    for path in sorted(HERE.rglob("*.py")):
        compile(path.read_text(), str(path), "exec")
    for path in sorted(HERE.rglob("*.mjs")):
        subprocess.run(["node", "--check", str(path)], check=True)
    runners = {
        "node": ["workload.mjs", "node.mjs"],
        "browser": ["workload.mjs", "browser-page.mjs"],
        "swift": ios.SOURCES,
        "kotlin": android.SOURCES,
    }
    for host, names in runners.items():
        source = ""
        for name in names:
            matches = list((HERE / "hosts").rglob(name))
            if len(matches) != 1:
                raise BenchError(f"{host}: expected one hosts/**/{name}")
            source += matches[0].read_text()
        # Each runner handles every workload and pages the whole fixture.
        missing = [w for w in WORKLOADS if f'"{w}"' not in source]
        if missing or f"{ROWS}" not in source:
            raise BenchError(f"{host}: runner misses {missing or 'the page size'}")
    gradle = (HERE / "hosts/android/build.gradle").read_text()
    manifest = (HERE / "hosts/android/src/main/AndroidManifest.xml").read_text()
    if any(f"'{name}'" not in gradle for name in android.SOURCES):
        raise BenchError("build.gradle does not compile every Kotlin runner")
    if f'"{android.PACKAGE}.Benchmark"' not in manifest:
        raise BenchError("AndroidManifest.xml does not name the instrumentation")
    pbxproj = ios.project(Path("/placeholder/XmtpSdk"))
    if any(f'path = "{name}"' not in pbxproj for name in ios.SOURCES):
        raise BenchError("The Xcode project does not compile every Swift runner")
    print("Benchmark runners pass the static checks")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("host", choices=(*HOSTS, "check"), help="host, or check")
    parser.add_argument(
        "--output", help="new directory (default target/sdk-bench/<host>-<time>)"
    )
    parser.add_argument("--samples", type=int, default=5, help="samples per workload")
    parser.add_argument(
        "--timeout", type=float, default=900, help="seconds per host call"
    )
    parser.add_argument(
        "--simulator", help="iOS Simulator UDID (default: the booted one)"
    )
    parser.add_argument("--device", help="adb serial (default: the only device)")
    parser.add_argument(
        "--keep-state", action="store_true", help="keep client databases"
    )
    args = parser.parse_args()
    if args.samples < 1:
        parser.error("--samples must be positive")
    if args.host == "check":
        check()
    else:
        run(args.host, args)


if __name__ == "__main__":
    try:
        main()
    except (BenchError, OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"Benchmark failed: {error}", file=sys.stderr)
        sys.exit(1)
