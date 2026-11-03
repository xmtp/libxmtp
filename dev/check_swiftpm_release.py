#!/usr/bin/env python3
"""Install a prepared SwiftPM release tree using its exact checked archive."""

import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


def check(archive, plan_path):
    record = json.loads(plan_path.read_text())
    root = Path(__file__).resolve().parent.parent
    with tempfile.TemporaryDirectory(prefix="xmtp-swiftpm-release-") as folder:
        checkout = Path(folder) / "package"
        subprocess.run(
            [
                "git",
                "worktree",
                "add",
                "--detach",
                str(checkout),
                "ios-" + record["version"],
            ],
            cwd=root,
            check=True,
        )
        try:
            if (checkout / "sdks/ios/Artifacts/XmtpSdkFFI.xcframework").exists():
                raise ValueError(
                    "Release installation must not use developer artifacts"
                )
            receipt = json.loads(
                (checkout / "sdks/ios/ReleaseArtifacts.json").read_text()
            )
            digest = hashlib.sha256(archive.read_bytes()).hexdigest()
            expected_url = f"https://github.com/{os.environ['GITHUB_REPOSITORY']}/releases/download/libxmtp-ios-{record['version']}/XmtpSdkFFI.zip"
            if receipt != dict(
                url=expected_url,
                sha256=digest,
                sourceSha=record["sourceSha"],
                version=record["version"],
            ):
                raise ValueError(
                    "SwiftPM receipt differs from the checked source/version/archive"
                )
            manifest = json.loads(
                subprocess.check_output(
                    [
                        "swift",
                        "package",
                        "--package-path",
                        str(checkout),
                        "dump-package",
                    ],
                    text=True,
                )
            )
            binary = next(
                target
                for target in manifest["targets"]
                if target["name"] == "xmtp_sdkFFI"
            )
            if binary.get("url") != receipt["url"] or binary.get("checksum") != digest:
                raise ValueError(
                    "SwiftPM manifest does not consume the checked release receipt"
                )
            # The remote URL cannot exist during a dry run. Substitute only
            # transport in this disposable tree; SwiftPM must unpack the actual
            # archive, locate its XCFramework, link it, and execute sdkVersion().
            local = checkout / "sdks/ios/ReleaseArtifact.zip"
            shutil.copyfile(archive, local)
            package = checkout / "Package.swift"
            text = package.read_text()
            remote = 'binary = .binaryTarget(name: "xmtp_sdkFFI", url: url, checksum: checksum)'
            if text.count(remote) != 1:
                raise ValueError("SwiftPM remote binary declaration changed")
            package.write_text(
                text.replace(
                    remote,
                    'binary = .binaryTarget(name: "xmtp_sdkFFI", path: "sdks/ios/ReleaseArtifact.zip")',
                )
            )
            subprocess.run(
                ["bash", str(checkout / "sdks/ios/script/check-consumer.sh")],
                cwd=checkout,
                env=dict(os.environ, XMTP_SDK_RELEASE_VERSION=record["version"]),
                check=True,
            )
        finally:
            subprocess.run(
                ["git", "worktree", "remove", "--force", str(checkout)],
                cwd=root,
                check=True,
            )


if __name__ == "__main__":
    check(Path(sys.argv[1]).resolve(), Path(sys.argv[2]).resolve())
