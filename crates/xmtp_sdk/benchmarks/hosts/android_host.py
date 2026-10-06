"""Build the release benchmark APK and run one request per instrumentation."""

import contextlib
import hashlib
import json
import subprocess
from pathlib import Path
from urllib.parse import urlsplit

HOSTS = Path(__file__).resolve().parent
ROOT = HOSTS.parents[3]
PACKAGE = "org.xmtp.benchmark"
REMOTE = f"/sdcard/Android/data/{PACKAGE}/files/benchmark-input"
# setgid + rwx for all: the app writes, the app's files inherit the group.
INPUT_MODE = "2777"
SOURCES = ["Benchmark.kt", "Sdk.kt"]
# Dependency locks and checksums. The build runs with strict verification.
DEPENDENCY_INPUTS = [
    "gradle.lockfile",
    "buildscript-gradle.lockfile",
    "gradle/verification-metadata.xml",
]
CLEANUP_SECONDS = 15
LOOPBACK = {"localhost", "127.0.0.1", "::1"}
DEFAULT_PORTS = {"http": 80, "https": 443}


def adb(config, *args, timeout=120):
    return subprocess.run(
        ["adb", "-s", config["serial"], *args],
        text=True,
        capture_output=True,
        check=True,
        timeout=timeout,
    )


def only_device():
    lines = subprocess.run(
        ["adb", "devices"], capture_output=True, text=True, check=True
    ).stdout.splitlines()[1:]
    devices = [line.split()[0] for line in lines if line.endswith("\tdevice")]
    if len(devices) != 1:
        raise ValueError(f"Connect one device or pass --device ({len(devices)} found)")
    return devices[0]


def check_dependency_inputs():
    for name in DEPENDENCY_INPUTS:
        if not (HOSTS / "android" / name).is_file():
            raise ValueError(f"Android dependency input missing: {name}")


def build_apk(aar, output):
    """Assemble the release APK against the staged AAR. Returns the APK path."""
    check_dependency_inputs()
    output.mkdir(parents=True)
    argv = [
        str(ROOT / "sdks/android/gradlew"),
        "-p",
        str(HOSTS / "android"),
        "assembleRelease",
        f"-PsdkAar={aar}",
        f"-PbenchmarkBuildDirectory={output}",
        "--dependency-verification=strict",
        "--no-daemon",
    ]
    with (output / "build.log").open("w") as log:
        result = subprocess.run(argv, stdout=log, stderr=subprocess.STDOUT)
    if result.returncode:
        raise ValueError(f"Android APK build failed. See {output / 'build.log'}")
    apks = list((output / "outputs/apk/release").glob("*.apk"))
    if len(apks) != 1:
        raise ValueError(f"Expected one release APK in {output}")
    return apks[0]


def backend_ports(url):
    """Return the host ports that the app reaches through a loopback backend URL.

    A remote backend needs no reverse. A loopback URL without a port uses the
    default port of its scheme.
    """
    parts = urlsplit(url)
    if parts.hostname not in LOOPBACK:
        return []
    port = parts.port or DEFAULT_PORTS.get(parts.scheme)
    if port is None:
        raise ValueError(
            f"Backend URL has no port and no default for its scheme: {url}"
        )
    return [port]


@contextlib.contextmanager
def reverse(config, ports):
    """Let the app reach the host's signer and backend at 127.0.0.1."""
    if not all(isinstance(port, int) and 0 < port < 65536 for port in ports):
        raise ValueError(f"adb reverse needs explicit TCP ports: {ports}")
    for port in ports:
        adb(config, "reverse", f"tcp:{port}", f"tcp:{port}")
    try:
        yield
    finally:
        for port in ports:
            subprocess.run(
                ["adb", "-s", config["serial"], "reverse", "--remove", f"tcp:{port}"]
            )


def install(config):
    adb(config, "install", "-r", config["apk"], timeout=config["timeout_seconds"])


def force_stop(config):
    """Stop the app on the device. A timeout kills only the local adb client,
    so an instrumentation can still run there and disturb the next request.
    Best effort: a failure here must not hide the result of the request.
    """
    try:
        subprocess.run(
            ["adb", "-s", config["serial"], "shell", "am", "force-stop", PACKAGE],
            capture_output=True,
            timeout=CLEANUP_SECONDS,
        )
    except (OSError, subprocess.SubprocessError):
        pass


def invoke(config, request, log):
    root = Path(request["state_directory"])
    # The app keeps its databases in its private files directory under this key.
    key = "bench-" + hashlib.sha256(str(root).encode()).hexdigest()[:24]
    adb(config, "shell", "mkdir", "-p", REMOTE)
    # A shell-created directory must permit the release app to write its result.
    # Keep the setgid bit that mkdir inherits from the app's files directory:
    # then response.json gets the ext_data_rw group, which the shell user can
    # read. A plain 0777 clears it, and on API 36 `cat` is then denied.
    adb(config, "shell", "chmod", INPUT_MODE, REMOTE)
    local = log.with_suffix(".request.json")
    local.write_text(json.dumps({**request, "state_key": key}))
    host = log.with_suffix(".host.json")
    host.write_text(json.dumps(config["host"]))
    for source, name in [
        (local, "request.json"),
        (host, "host.json"),
        (root / "fixture.json", "fixture.json"),
    ]:
        adb(config, "push", str(source), f"{REMOTE}/{name}")
    adb(config, "shell", "rm", "-f", f"{REMOTE}/response.json")
    try:
        run = adb(
            config,
            "shell",
            "am",
            "instrument",
            "-w",
            f"{PACKAGE}/{PACKAGE}.Benchmark",
            timeout=config["timeout_seconds"],
        )
    finally:
        force_stop(config)
    output = log.with_suffix(".instrumentation.log")
    output.write_text(run.stdout + run.stderr)
    if (
        "benchmark=complete" not in run.stdout
        or "INSTRUMENTATION_CODE: 0" not in run.stdout
    ):
        raise RuntimeError(f"Android benchmark failed. See {output}")
    return json.loads(adb(config, "shell", "cat", f"{REMOTE}/response.json").stdout)
