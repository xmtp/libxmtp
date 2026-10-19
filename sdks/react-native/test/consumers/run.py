#!/usr/bin/env python3
"""Launch built native consumers and require fresh sibling-verified assertions."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import selectors
import shlex
import shutil
import stat
import subprocess
import sys
import tempfile
import time
from urllib.parse import urlencode, urlparse

from build import HERE, sdk_identity, harness_identity


def command(args, **kwargs):
    return subprocess.run(args, check=True, **kwargs)


def wait_ready(peer, timeout):
    selector = selectors.DefaultSelector()
    selector.register(peer.stdout, selectors.EVENT_READ)
    deadline = time.monotonic() + timeout
    buffered = b""
    try:
        while True:
            remaining = deadline - time.monotonic()
            if remaining <= 0 or not selector.select(remaining):
                raise RuntimeError("sibling peer did not start before the deadline")
            chunk = os.read(peer.stdout.fileno(), 65536)
            if not chunk:
                raise RuntimeError(
                    f"sibling peer exited before readiness ({peer.poll()})"
                )
            buffered += chunk
            while b"\n" in buffered:
                line, buffered = buffered.split(b"\n", 1)
                try:
                    ready = json.loads(line)
                except (json.JSONDecodeError, UnicodeDecodeError):
                    continue
                if (
                    isinstance(ready, dict)
                    and isinstance(ready.get("port"), int)
                    and isinstance(ready.get("token"), str)
                ):
                    return ready
    finally:
        selector.close()


def journey_set():
    """`pr` (the default) runs the RN-only journeys; `full` adds runtime reload and cold background."""
    journeys = os.environ.get("XMTP_RN_JOURNEYS") or "pr"
    if journeys not in ("pr", "full"):
        raise ValueError("XMTP_RN_JOURNEYS must be pr or full")
    return journeys


def required_phases(mode):
    return {
        "foundation": ["foundation"],
        "conformance": [
            "foundation",
            "content",
            "boundary",
            "listener",
            "react",
            *(["runtime-reload"] if journey_set() == "full" else []),
        ],
        "background": ["cold-background"],
    }[mode]


def verify_report(report, mode, expected_harness, platform):
    if report.get("status") != "passed" or report.get("platform") != platform:
        raise RuntimeError(
            f"native assertions failed: {report.get('message', 'wrong platform/status')}"
        )
    if report.get("mode") != mode or report.get("phases") != required_phases(mode):
        raise RuntimeError(
            "native result omitted the requested mode or completed phase witnesses"
        )
    if report.get("harnessSha256") != expected_harness:
        raise RuntimeError("native result came from a different installed harness")
    if not isinstance(report.get("assertions"), list) or not report["assertions"]:
        raise RuntimeError("native result omitted assertion evidence")


def resolve_simulator(selector):
    inventory = json.loads(
        subprocess.check_output(
            ["/usr/bin/xcrun", "simctl", "list", "devices", "--json"], text=True
        )
    )
    matches = [
        device
        for devices in inventory["devices"].values()
        for device in devices
        if device.get("isAvailable", False)
        and (
            device["state"] == "Booted"
            if selector == "booted"
            else device["udid"].lower() == selector.lower()
            or device["name"] == selector
        )
    ]
    if len(matches) != 1:
        raise ValueError(
            f"simulator selector {selector!r} matched {len(matches)} available devices; pass an exact device UUID"
        )
    return matches[0]["udid"]


def simulator_pid(output, application_id):
    matches = re.findall(rf"^{re.escape(application_id)}: (\d+)$", output, re.MULTILINE)
    if len(matches) != 1:
        raise RuntimeError("simctl did not report exactly one fixture process")
    return int(matches[0])


def verify_fixture_process(pid, device, application):
    executable = subprocess.check_output(
        ["/bin/ps", "-p", str(pid), "-o", "comm="], text=True
    ).strip()
    if f"/Devices/{device}/" not in executable or not executable.endswith(
        f"/{application.name}/{application.stem}"
    ):
        raise RuntimeError("refusing to attach debugger to an unrelated process")


def android_pid(adb, application):
    result = subprocess.run(
        adb + ["shell", "pidof", application], capture_output=True, text=True
    )
    if result.returncode not in (0, 1):
        raise RuntimeError("could not read the owned Android process")
    value = result.stdout.strip()
    if value and not value.isdecimal():
        raise RuntimeError("expected exactly one owned Android process")
    return int(value) if value else None


def application_inventory(application):
    application = application.resolve(strict=True)
    if not application.is_dir() or application.suffix != ".app":
        raise ValueError("iOS install source must be an application bundle")
    entries = []
    for path in [application, *sorted(application.rglob("*"))]:
        entry = {"path": path.relative_to(application).as_posix()}
        if path.is_symlink():
            if not path.resolve(strict=True).is_relative_to(application):
                raise ValueError("iOS application contains an external symbolic link")
            entry.update(type="symlink", target=str(path.readlink()))
        elif path.is_file():
            with path.open("rb") as stream:
                digest = hashlib.file_digest(stream, "sha256").hexdigest()
            entry.update(type="file", bytes=path.stat().st_size, sha256=digest)
        elif path.is_dir():
            entry.update(type="directory")
        else:
            raise ValueError("iOS application contains an unsupported file type")
        entries.append(entry)
    return entries


def stage_ios_application(application, directory):
    directory = directory.resolve(strict=True)
    if not directory.is_dir() or directory.stat().st_uid != os.getuid():
        raise ValueError("iOS staging requires an owned temporary directory")
    original = application_inventory(application)
    staged = directory / application.name
    shutil.copytree(application, staged, symlinks=True)
    # CoreSimulator cannot recursively copy a read-only Nix bundle. Change
    # only owner-write permissions on the verified, disposable copy.
    for path in [staged, *staged.rglob("*")]:
        if not path.is_symlink():
            path.chmod(stat.S_IMODE(path.stat().st_mode) | stat.S_IWUSR)
    if (
        application_inventory(application) != original
        or application_inventory(staged) != original
    ):
        raise ValueError("staged iOS application differs from the admitted bundle")
    digest = hashlib.sha256(
        json.dumps(original, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()
    return staged, digest


def execute(args, consumer, package, output, background=False):
    receipt = json.loads((output / "consumer-build.json").read_text())
    expected_harness = harness_identity(consumer)
    if receipt.get("harnessSha256") != expected_harness:
        raise ValueError("built consumer differs from the currently admitted harness")
    if args.platform == "ios" and (
        os.environ.get("XMTP_RN_XCODE_MODE") not in {"local", "canonical"}
        or not os.environ.get("XMTP_RN_XCODE_IDENTITY")
    ):
        raise ValueError(
            "iOS runtime proof requires an explicit selected Xcode identity"
        )
    if args.platform == "ios" and (
        receipt.get("xcodeMode"),
        receipt.get("xcodeIdentity"),
    ) != (
        os.environ.get("XMTP_RN_XCODE_MODE"),
        os.environ.get("XMTP_RN_XCODE_IDENTITY"),
    ):
        raise ValueError(
            "built iOS consumer and runner selected different local/canonical Xcode identities"
        )
    mode = (
        "background"
        if background
        else "conformance"
        if args.conformance
        else "foundation"
    )
    if receipt["packageSha256"] != sdk_identity(
        package, args.expected_version, args.expected_source
    ):
        raise ValueError("built consumer and installed SDK package identities differ")
    if (receipt["expectedVersion"], receipt["expectedSource"]) != (
        args.expected_version,
        args.expected_source,
    ):
        raise ValueError("built consumer and requested release identities differ")
    if (receipt["consumer"], receipt["platform"], receipt["device"]) != (
        consumer,
        args.platform,
        False,
    ):
        raise ValueError(
            "runtime proof requires the selected simulator/emulator consumer build"
        )
    app = output / receipt["application"]
    with tempfile.TemporaryDirectory(prefix=f"xmtp-rn-{consumer}-") as temporary:
        if args.platform == "ios":
            app, application_content_sha256 = stage_ios_application(
                app, Path(temporary)
            )
        result = Path(temporary) / "result.json"
        env = os.environ.copy()
        env["XMTP_RN_RESULT"] = str(result)
        env["XMTP_RN_REQUESTED_MODE"] = mode
        env["XMTP_RN_EXPECTED_HARNESS"] = expected_harness
        env["XMTP_RN_REQUIRED_PHASES_JSON"] = json.dumps(required_phases(mode))
        phase = Path(temporary) / "background-phase.json"
        if background:
            env["XMTP_RN_BACKGROUND_PHASE"] = str(phase)
        with (args.results / f"{consumer}-peer.log").open("w") as peer_log:
            peer = subprocess.Popen(
                ["node", str(HERE.parent / "conformance/peer.mjs")],
                stdout=subprocess.PIPE,
                stderr=peer_log,
                text=True,
                env=env,
            )
            forwards = []
            background_pid = None
            foreground_pid = None
            prior_android_pid = None
            installed_apk_sha256 = None
            try:
                ready = wait_ready(peer, args.timeout)
                endpoint = f"http://127.0.0.1:{ready['port']}"
                url = "xmtprn://run?" + urlencode(
                    {
                        "control": endpoint,
                        "token": ready["token"],
                        "mode": mode,
                    }
                )
                if args.platform == "android":
                    adb = ["adb"] + (["-s", args.device] if args.device else [])
                    command(adb + ["wait-for-device"])
                    backend = urlparse(env["XMTP_BACKEND_URL"])
                    if backend.hostname not in ("localhost", "127.0.0.1", "::1"):
                        raise ValueError(
                            "local Android proof requires the worktree backend loopback URL"
                        )
                    ports = {
                        ready["port"],
                        backend.port or (443 if backend.scheme == "https" else 80),
                    }
                    for name in ("XMTP_S3_URL", "XMTP_S3_BASE_URL"):
                        if value := env.get(name):
                            endpoint_url = urlparse(value)
                            if endpoint_url.scheme not in ("http", "https"):
                                raise ValueError(f"{name} requires an HTTP(S) URL")
                            if endpoint_url.hostname in (
                                "localhost",
                                "127.0.0.1",
                                "::1",
                            ):
                                ports.add(
                                    endpoint_url.port
                                    or (443 if endpoint_url.scheme == "https" else 80)
                                )
                    for port in sorted(ports):
                        command(adb + ["reverse", f"tcp:{port}", f"tcp:{port}"])
                        forwards.append(port)
                    if background:
                        main_report = json.loads(
                            (args.results / f"{consumer}-android.json").read_text()
                        )
                        verify_report(
                            main_report, "conformance", expected_harness, "android"
                        )
                        if main_report.get("build") != receipt:
                            raise RuntimeError(
                                "background APK lacks matching completed main proof"
                            )
                        paths = subprocess.check_output(
                            adb + ["shell", "pm", "path", receipt["applicationId"]],
                            text=True,
                        ).splitlines()
                        if len(paths) != 1 or not paths[0].startswith("package:"):
                            raise RuntimeError(
                                "expected one installed canonical Android APK"
                            )
                        installed_apk_sha256 = subprocess.check_output(
                            adb + ["shell", "sha256sum", shlex.quote(paths[0][8:])],
                            text=True,
                        ).split()[0]
                        with app.open("rb") as stream:
                            expected_apk_sha256 = hashlib.file_digest(
                                stream, "sha256"
                            ).hexdigest()
                        if installed_apk_sha256 != expected_apk_sha256:
                            raise RuntimeError(
                                "installed background APK differs from canonical APK"
                            )
                        prior_android_pid = android_pid(adb, receipt["applicationId"])
                    else:
                        subprocess.run(
                            adb
                            + ["shell", "am", "force-stop", receipt["applicationId"]],
                            check=False,
                            capture_output=True,
                        )
                        subprocess.run(
                            adb + ["uninstall", receipt["applicationId"]],
                            check=False,
                            capture_output=True,
                        )
                        command(adb + ["install", str(app)])
                    command(
                        adb + ["shell", "am", "force-stop", receipt["applicationId"]]
                    )
                    if background:
                        if android_pid(adb, receipt["applicationId"]) is not None:
                            raise RuntimeError(
                                "owned Android process survived force-stop"
                            )
                        command(
                            adb
                            + [
                                "shell",
                                "am",
                                "broadcast",
                                "--include-stopped-packages",
                                "-n",
                                receipt["applicationId"]
                                + "/com.xmtp.consumer.fixture.XmtpBackgroundReceiver",
                                "--es",
                                "url",
                                shlex.quote(url),
                            ]
                        )
                        background_pid = android_pid(adb, receipt["applicationId"])
                        if (
                            background_pid is None
                            or background_pid == prior_android_pid
                        ):
                            raise RuntimeError(
                                "background launch did not create a fresh Android process"
                            )
                    else:
                        command(
                            adb
                            + [
                                "shell",
                                "am",
                                "start",
                                "-W",
                                "-a",
                                "android.intent.action.VIEW",
                                "-d",
                                shlex.quote(url),
                                receipt["applicationId"],
                            ]
                        )
                else:
                    device = resolve_simulator(args.device or "booted")
                    command(["/usr/bin/xcrun", "simctl", "bootstatus", device, "-b"])
                    subprocess.run(
                        [
                            "/usr/bin/xcrun",
                            "simctl",
                            "uninstall",
                            device,
                            receipt["applicationId"],
                        ],
                        check=False,
                        capture_output=True,
                    )
                    command(["/usr/bin/xcrun", "simctl", "install", device, str(app)])
                    subprocess.run(
                        [
                            "/usr/bin/xcrun",
                            "simctl",
                            "terminate",
                            device,
                            receipt["applicationId"],
                        ],
                        check=False,
                        capture_output=True,
                    )
                    if background:
                        # The private native-only launch defers all React/SDK startup.
                        # Moving real OS focus triggers its first initialization in Background.
                        launched = command(
                            [
                                "/usr/bin/xcrun",
                                "simctl",
                                "launch",
                                device,
                                receipt["applicationId"],
                                "--xmtp-background",
                                url,
                            ],
                            capture_output=True,
                            text=True,
                            timeout=45,
                        )
                        background_pid = simulator_pid(
                            launched.stdout, receipt["applicationId"]
                        )
                        print(
                            f"Fresh native-only fixture PID: {background_pid}",
                            flush=True,
                        )
                        verify_fixture_process(background_pid, device, app)
                        command(
                            [
                                "/usr/bin/xcrun",
                                "simctl",
                                "launch",
                                device,
                                "com.apple.Preferences",
                            ],
                            timeout=45,
                        )
                    else:
                        command(
                            [
                                "/usr/bin/xcrun",
                                "simctl",
                                "launch",
                                device,
                                receipt["applicationId"],
                                "--xmtp-consumer",
                                url,
                            ]
                        )
                deadline = time.monotonic() + args.timeout
                foregrounded = False
                while not result.exists():
                    if background and phase.exists() and not foregrounded:
                        if json.loads(phase.read_text()) != {"action": "foreground"}:
                            raise RuntimeError("unknown native background action")
                        if args.platform == "android":
                            command(
                                adb
                                + [
                                    "shell",
                                    "am",
                                    "start",
                                    "-W",
                                    "-n",
                                    receipt["applicationId"] + "/.MainActivity",
                                ]
                            )
                            foreground_pid = android_pid(adb, receipt["applicationId"])
                            if foreground_pid != background_pid:
                                raise RuntimeError(
                                    "OS foreground replaced the Android background process"
                                )
                        else:
                            launched = command(
                                [
                                    "/usr/bin/xcrun",
                                    "simctl",
                                    "launch",
                                    device,
                                    receipt["applicationId"],
                                ],
                                capture_output=True,
                                text=True,
                            )
                            foreground_pid = simulator_pid(
                                launched.stdout, receipt["applicationId"]
                            )
                            if foreground_pid != background_pid:
                                raise RuntimeError(
                                    "OS foreground replaced the background initialization process"
                                )
                        foregrounded = True
                    if peer.poll() is not None:
                        raise RuntimeError(
                            "sibling peer exited before native assertions completed"
                        )
                    if time.monotonic() >= deadline:
                        raise RuntimeError(
                            "native app did not report assertions before the deadline"
                        )
                    time.sleep(0.25)
                report = json.loads(result.read_text())
                report["build"] = receipt
                if args.platform == "ios":
                    report["applicationContentSha256"] = application_content_sha256
                if background and args.platform == "android":
                    if report.get("runId") == main_report.get("runId"):
                        raise RuntimeError(
                            "background result reused the main run identity"
                        )
                    report["installedApkSha256"] = installed_apk_sha256
                    report["previousProcessId"] = prior_android_pid
                    report["backgroundProcessId"] = background_pid
                    report["foregroundProcessId"] = foreground_pid
                    report["backgroundTransport"] = (
                        "fresh-headless-process-with-verified-canonical-apk"
                    )
                if background and args.platform == "ios":
                    report["backgroundTransport"] = "fresh-react-sdk-in-os-background"
                    report["backgroundProcessId"] = background_pid
                    report["foregroundProcessId"] = foreground_pid
                (
                    args.results
                    / f"{consumer}-{args.platform}{'-background' if background else ''}.json"
                ).write_text(json.dumps(report, indent=2) + "\n")
                verify_report(report, mode, expected_harness, args.platform)
                print(
                    f"{consumer}/{args.platform}: {len(report['assertions'])} native assertions passed",
                    flush=True,
                )
            finally:
                if peer.poll() is None:
                    peer.terminate()
                    try:
                        peer.wait(timeout=20)
                    except subprocess.TimeoutExpired:
                        peer.kill()
                        peer.wait()
                if args.platform == "android":
                    for port in forwards:
                        subprocess.run(
                            adb + ["reverse", "--remove", f"tcp:{port}"],
                            check=False,
                            capture_output=True,
                        )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("platform", choices=["android", "ios"])
    parser.add_argument(
        "selection", choices=["bare", "expo", "all"], nargs="?", default="all"
    )
    parser.add_argument("--conformance", action="store_true")
    parser.add_argument(
        "--package", type=Path, default=os.environ.get("XMTP_REACT_NATIVE_PACKAGE")
    )
    parser.add_argument(
        "--built-root", type=Path, default=os.environ.get("XMTP_RN_CONSUMER_BUNDLES")
    )
    parser.add_argument(
        "--results", type=Path, default=Path("target/react-native-results")
    )
    parser.add_argument("--device", default=os.environ.get("XMTP_RN_DEVICE"))
    parser.add_argument("--timeout", type=int, default=180)
    parser.add_argument(
        "--expected-version",
        default=os.environ.get("XMTP_RN_EXPECTED_VERSION", "8.0.0"),
    )
    parser.add_argument(
        "--expected-source", default=os.environ.get("XMTP_RN_EXPECTED_SOURCE")
    )
    parser.add_argument("--offline", action="store_true")
    args = parser.parse_args()
    if os.environ.get("XMTP_NIX_ENV") != "yes" and not os.environ.get("IN_NIX_SHELL"):
        parser.error("run installed-package proofs through dev/nix-shell")
    try:
        background = args.conformance and journey_set() == "full"
    except ValueError as error:
        parser.error(str(error))
    if not os.environ.get("XMTP_NODE_SDK_DIR") or not os.environ.get(
        "XMTP_BACKEND_URL"
    ):
        parser.error(
            "XMTP_NODE_SDK_DIR and XMTP_BACKEND_URL must select the staged sibling and live worktree backend"
        )
    if args.package is None:
        package_directory = Path(
            os.environ.get("XMTP_RN_PACKAGE_DIR", "target/rn-release/package")
        )
        candidates = [
            path
            for path in [
                package_directory / "package.tgz",
                package_directory
                / f"xmtp-react-native-sdk-{args.expected_version}.tgz",
            ]
            if path.is_file()
        ]
        candidates = list({path.resolve(): path for path in candidates}.values())
        if len(candidates) != 1:
            parser.error(
                "pass the exact canonical tarball using --package or XMTP_REACT_NATIVE_PACKAGE"
            )
        args.package = candidates[0]
    package = args.package.resolve(strict=True)
    sdk_identity(package, args.expected_version, args.expected_source)
    args.results = args.results.resolve()
    args.results.mkdir(parents=True, exist_ok=True)
    for consumer in ["bare", "expo"] if args.selection == "all" else [args.selection]:
        if args.built_root:
            execute(args, consumer, package, args.built_root.resolve() / consumer)
            if background:
                execute(
                    args,
                    consumer,
                    package,
                    args.built_root.resolve() / consumer,
                    background=True,
                )
        else:
            with tempfile.TemporaryDirectory(prefix="xmtp-rn-build-") as temporary:
                root = Path(temporary)
                build_command = [
                    sys.executable,
                    str(HERE / "build.py"),
                    "--consumer",
                    consumer,
                    "--platform",
                    args.platform,
                    "--package",
                    str(package),
                    "--work-dir",
                    str(root / "work"),
                    "--output",
                    str(root / "output"),
                    "--expected-version",
                    args.expected_version,
                    "--expected-source",
                    args.expected_source,
                ]
                if args.offline:
                    build_command.append("--offline")
                command(build_command)
                execute(args, consumer, package, root / "output")
                if background:
                    execute(args, consumer, package, root / "output", background=True)


if __name__ == "__main__":
    main()
