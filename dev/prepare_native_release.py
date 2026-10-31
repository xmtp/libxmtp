#!/usr/bin/env python3
"""Bind locally prepared native products to immutable source-checked tags."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
from urllib.request import urlopen
import zipfile

from sdk_version import validate_record


def git(root, *args):
    return subprocess.check_output(["git", "-C", str(root), *args], text=True).strip()


def validate_commit(root, commit, sdk, source):
    if commit == source:
        return
    if (
        sdk != "ios"
        or git(root, "rev-list", "--parents", "-n", "1", commit) != f"{commit} {source}"
    ):
        raise ValueError("Native tag does not match the frozen source")
    paths = git(root, "diff", "--name-only", source, commit).splitlines()
    if any(
        path not in ("Package.swift", "sdks/ios/ReleaseArtifacts.json")
        and not path.startswith("sdks/ios/Sources/XmtpSdk/")
        for path in paths
    ):
        raise ValueError("iOS release commit changes files outside release paths")


def receipt(root, commit, record):
    data = json.loads(git(root, "show", f"{commit}:sdks/ios/ReleaseArtifacts.json"))
    url = (
        f"https://github.com/{os.environ['GITHUB_REPOSITORY']}/releases/download/"
        f"libxmtp-ios-{record['version']}/XmtpSdkFFI.zip"
    )
    if (
        data
        != dict(
            url=url,
            sha256=data.get("sha256"),
            sourceSha=record["sourceSha"],
            version=record["version"],
        )
        or not isinstance(data.get("sha256"), str)
        or len(data["sha256"]) != 64
    ):
        raise ValueError("iOS receipt differs from frozen source/version identity")
    return data


def archive_files(path):
    with zipfile.ZipFile(path) as archive:
        entries = archive.infolist()
        if not entries or len({entry.filename for entry in entries}) != len(entries):
            raise ValueError("Native archive has missing or duplicate entries")
        files = {}
        for entry in entries:
            with archive.open(entry) as content:
                files[entry.filename] = (
                    entry.external_attr,
                    hashlib.file_digest(content, "sha256").hexdigest(),
                )
        return files


def prepare(root, sdk, record, bundle):
    validate_record(record)
    source = record["sourceSha"]
    current = git(root, "rev-parse", "HEAD")
    validate_commit(root, current, sdk, source)
    if sdk == "ios":
        current_receipt = receipt(root, current, record)
        archive = root / "target/release/ios/XmtpSdkFFI.zip"
        if (
            hashlib.sha256(archive.read_bytes()).hexdigest()
            != current_receipt["sha256"]
        ):
            raise ValueError("Fresh SwiftPM archive differs from its release receipt")
        current_files = archive_files(archive)
    ref = f"refs/tags/{sdk}-{record['version']}"
    git(root, "check-ref-format", ref)
    exists = subprocess.run(
        ["git", "-C", str(root), "show-ref", "--verify", "--quiet", ref],
        check=False,
    )
    if exists.returncode not in (0, 1):
        raise ValueError("Cannot inspect native release tag")
    if exists.returncode == 0:
        previous = git(root, "rev-parse", "--verify", f"{ref}^{{commit}}")
        validate_commit(root, previous, sdk, source)
        if sdk == "ios":
            previous_receipt = receipt(root, previous, record)
            git(
                root,
                "diff",
                "--exit-code",
                previous,
                current,
                "--",
                "Package.swift",
                "sdks/ios/Sources/XmtpSdk",
            )
            binary = git(
                root,
                "rev-parse",
                "--verify",
                f"refs/tags/libxmtp-ios-{record['version']}^{{commit}}",
            )
            if binary != source:
                raise ValueError("Existing iOS binary tag differs from frozen source")
            with tempfile.TemporaryDirectory(prefix="xmtp-native-retry-") as folder:
                existing = Path(folder) / "XmtpSdkFFI.zip"
                with (
                    urlopen(previous_receipt["url"]) as response,
                    existing.open("wb") as output,
                ):
                    shutil.copyfileobj(response, output)
                if (
                    hashlib.sha256(existing.read_bytes()).hexdigest()
                    != previous_receipt["sha256"]
                ):
                    raise ValueError(
                        "Existing SwiftPM archive differs from its release receipt"
                    )
                if archive_files(existing) != current_files:
                    raise ValueError(
                        "Existing SwiftPM archive differs from the fresh native build"
                    )
                shutil.copyfile(existing, archive)
    else:
        git(root, "tag", ref.removeprefix("refs/tags/"))
    git(root, "bundle", "create", str(bundle), ref)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("sdk", choices=("android", "ios"))
    parser.add_argument("--plan", type=Path, required=True)
    parser.add_argument("--bundle", type=Path, required=True)
    args = parser.parse_args()
    prepare(
        Path(__file__).resolve().parent.parent,
        args.sdk,
        json.loads(args.plan.read_text()),
        args.bundle,
    )


if __name__ == "__main__":
    main()
