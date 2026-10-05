"""Build the release benchmark APK and run one request per instrumentation."""

import contextlib
import hashlib
import json
import subprocess
from pathlib import Path

HOSTS = Path(__file__).resolve().parent
ROOT = HOSTS.parents[3]
PACKAGE = "org.xmtp.benchmark"
REMOTE = f"/sdcard/Android/data/{PACKAGE}/files/benchmark-input"
SOURCES = ["Benchmark.kt", "Sdk.kt"]


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


def build_apk(aar, output):
    """Assemble the release APK against the staged AAR. Returns the APK path."""
    output.mkdir(parents=True)
    argv = [
        str(ROOT / "sdks/android/gradlew"),
        "-p",
        str(HOSTS / "android"),
        "assembleRelease",
        f"-PsdkAar={aar}",
        f"-PbenchmarkBuildDirectory={output}",
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


@contextlib.contextmanager
def reverse(config, ports):
    """Let the app reach the host's signer and backend at 127.0.0.1."""
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


def invoke(config, request, log):
    root = Path(request["state_directory"])
    # The app keeps its databases in its private files directory under this key.
    key = "bench-" + hashlib.sha256(str(root).encode()).hexdigest()[:24]
    adb(config, "shell", "mkdir", "-p", REMOTE)
    # A shell-created directory must permit the release app to write its result.
    adb(config, "shell", "chmod", "0777", REMOTE)
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
    run = adb(
        config,
        "shell",
        "am",
        "instrument",
        "-w",
        f"{PACKAGE}/{PACKAGE}.Benchmark",
        timeout=config["timeout_seconds"],
    )
    output = log.with_suffix(".instrumentation.log")
    output.write_text(run.stdout + run.stderr)
    if (
        "benchmark=complete" not in run.stdout
        or "INSTRUMENTATION_CODE: 0" not in run.stdout
    ):
        raise RuntimeError(f"Android benchmark failed. See {output}")
    return json.loads(adb(config, "shell", "cat", f"{REMOTE}/response.json").stdout)
