#!/usr/bin/env python3
"""Record the real pinned Gradle dependency graph for Nix's fetchDeps replay."""

import argparse
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import tempfile
import time


def run(command, cwd, **kwargs):
    subprocess.run(command, cwd=cwd, check=True, **kwargs)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--work-dir",
        type=Path,
        required=True,
        help="prepared genuine consumer app, including the installed SDK source package",
    )
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    work = args.work_dir.resolve(strict=True)
    output = args.output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    init = os.environ.get("XMTP_RN_GRADLE_INIT_DEPS")
    compress = os.environ.get("XMTP_RN_GRADLE_COMPRESS_DEPS")
    if not init or not compress:
        parser.error("run through the pinned react-native Nix shell")
    with tempfile.TemporaryDirectory(prefix="xmtp-rn-gradle-record-") as temporary:
        recording = Path(temporary)
        run(["openssl", "genrsa", "-out", "ca.key", "2048"], recording)
        run(
            [
                "openssl",
                "req",
                "-x509",
                "-new",
                "-nodes",
                "-key",
                "ca.key",
                "-sha256",
                "-days",
                "1",
                "-out",
                "ca.cer",
                "-subj",
                "/CN=XMTP native dependency capture",
            ],
            recording,
        )
        store = recording / "truststore"
        run(
            [
                "keytool",
                "-importcert",
                "-noprompt",
                "-file",
                "ca.cer",
                "-alias",
                "dependency-capture",
                "-keystore",
                str(store),
                "-storepass",
                "changeit",
            ],
            recording,
        )
        with socket.socket() as reservation:
            reservation.bind(("127.0.0.1", 0))
            port = reservation.getsockname()[1]
        with (output.with_suffix(".proxy.log")).open("w") as log:
            proxy = subprocess.Popen(
                [
                    "mitm-cache",
                    "-l",
                    f"127.0.0.1:{port}",
                    "record",
                    "--reject",
                    r"\.(md5|sha(1|256|512:?):?)$",
                    "--forget-redirects-from",
                    ".*",
                    "--record-text",
                    r"/maven-metadata\.xml$",
                ],
                cwd=recording,
                stdout=log,
                stderr=subprocess.STDOUT,
            )
            try:
                deadline = time.monotonic() + 20
                while True:
                    if proxy.poll() is not None:
                        raise RuntimeError(
                            "dependency capture proxy exited before readiness"
                        )
                    try:
                        with socket.create_connection(("127.0.0.1", port), timeout=0.2):
                            break
                    except OSError:
                        if time.monotonic() >= deadline:
                            raise RuntimeError(
                                "dependency capture proxy did not become ready"
                            )
                        time.sleep(0.1)
                env = os.environ.copy()
                env["GRADLE_USER_HOME"] = str(recording / "gradle-home")
                command = [
                    os.environ.get("XMTP_RN_GRADLE", "gradle"),
                    "--no-daemon",
                    "--init-script",
                    str(Path(__file__).with_name("capture-release.gradle")),
                    "-Dhttp.proxyHost=127.0.0.1",
                    f"-Dhttp.proxyPort={port}",
                    "-Dhttps.proxyHost=127.0.0.1",
                    f"-Dhttps.proxyPort={port}",
                    f"-Djavax.net.ssl.trustStore={store}",
                    "-Djavax.net.ssl.trustStorePassword=changeit",
                    "--write-verification-metadata",
                    "sha256",
                    "xmtpDownloadReleaseDependencies",
                ]
                with output.with_suffix(".gradle.log").open("w") as gradle_log:
                    run(
                        command,
                        work / "android",
                        env=env,
                        stdout=gradle_log,
                        stderr=subprocess.STDOUT,
                    )
            finally:
                if proxy.poll() is None:
                    proxy.send_signal(signal.SIGINT)
                    try:
                        proxy.wait(timeout=30)
                    except subprocess.TimeoutExpired:
                        proxy.kill()
                        proxy.wait()
            description = recording / "out.json"
            if not description.is_file():
                raise RuntimeError(
                    "capture proxy produced no immutable dependency description"
                )
            run(["python3.11", compress, str(description), str(output)], recording)
            if not json.loads(output.read_text()):
                raise RuntimeError("capture produced an empty native dependency graph")
            print(f"Recorded native dependency graph: {output}", flush=True)


if __name__ == "__main__":
    main()
