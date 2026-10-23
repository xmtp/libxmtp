#!/usr/bin/env python3
"""Publish exact checked npm bytes, or verify an identical prior publication."""

import base64
import hashlib
import json
import subprocess
import sys
import tarfile
from pathlib import Path


def check_package(tarball, version, sdk):
    with tarfile.open(tarball, "r:gz") as archive:
        package = json.load(archive.extractfile("package/package.json"))
    expected_name = "@xmtp/" + sdk if sdk != "cli" else "@xmtp/cli"
    if package["name"] != expected_name:
        raise ValueError("Package name differs from the requested SDK")
    if package["version"] != version:
        raise ValueError("Package version differs from the release plan")
    if (
        sdk in ("agent-sdk", "cli")
        and package["dependencies"]["@xmtp/node-sdk"] != version
    ):
        raise ValueError("Package must pin the matching Node version")
    return package


def publish(tarball, version, tag, sdk):
    package = check_package(tarball, version, sdk)
    if sdk in ("agent-sdk", "cli"):
        node = subprocess.run(
            ["npm", "view", "@xmtp/node-sdk@" + version, "version", "--json"],
            capture_output=True,
            text=True,
        )
        if node.returncode != 0 or json.loads(node.stdout) != version:
            raise ValueError("Matching Node SDK has not been published")
    integrity = (
        "sha512-"
        + base64.b64encode(hashlib.sha512(Path(tarball).read_bytes()).digest()).decode()
    )
    spec = package["name"] + "@" + version
    result = subprocess.run(
        ["npm", "view", spec, "dist.integrity", "--json"],
        capture_output=True,
        text=True,
    )
    if result.returncode == 0:
        if json.loads(result.stdout) != integrity:
            raise ValueError("Existing npm version has different bytes")
        print(f"Verified identical publication: {spec}")
        return
    try:
        code = json.loads(result.stdout)["error"]["code"]
    except (KeyError, ValueError):
        raise ValueError("Cannot determine whether npm version exists") from None
    if code != "E404":
        raise ValueError("npm version lookup failed: " + code)
    subprocess.run(
        [
            "npm",
            "publish",
            str(tarball),
            "--provenance",
            "--ignore-scripts",
            "--tag",
            tag,
        ],
        check=True,
    )


if __name__ == "__main__":
    try:
        if sys.argv[1] == "--check":
            check_package(*sys.argv[2:])
        else:
            publish(*sys.argv[1:])
    except ValueError as error:
        sys.exit(str(error))
