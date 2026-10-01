#!/usr/bin/env python3
"""Resolve published stable baseline metadata from the package owners."""

import argparse
import base64
import hashlib
import json
import re
import urllib.request
import xml.etree.ElementTree as ET
from datetime import datetime, timezone
from pathlib import Path

STABLE = re.compile(r"\d+\.\d+\.\d+")


def fetch(url):
    request = urllib.request.Request(
        url, headers={"User-Agent": "xmtp-cutover-benchmark"}
    )
    with urllib.request.urlopen(request, timeout=60) as response:
        return response.read()


def github(path):
    return json.loads(fetch("https://api.github.com/repos/xmtp/libxmtp/" + path))


def resolve():
    packages = {}
    for target in ("node", "browser"):
        name = f"@xmtp/{target}-sdk"
        url = "https://registry.npmjs.org/" + name.replace("/", "%2f")
        metadata = json.loads(fetch(url))
        _, version = max(
            (metadata["time"][v], v)
            for v in metadata["versions"]
            if STABLE.fullmatch(v)
        )
        package = metadata["versions"][version]
        tarball = fetch(package["dist"]["tarball"])
        integrity = (
            "sha512-" + base64.b64encode(hashlib.sha512(tarball).digest()).decode()
        )
        if integrity != package["dist"]["integrity"]:
            raise ValueError(f"Registry integrity mismatch for {name}")
        packages[target] = {
            "name": name,
            "version": version,
            "published": metadata["time"][version],
            "metadata_url": url,
            "commit": package["gitHead"],
            "archive_url": package["dist"]["tarball"],
            "integrity": integrity,
            "archive_sha256": hashlib.sha256(tarball).hexdigest(),
            "dependencies": package.get("dependencies", {}),
            "optional_dependencies": package.get("optionalDependencies", {}),
        }
    url = "https://repo.maven.apache.org/maven2/org/xmtp/android/maven-metadata.xml"
    metadata = ET.fromstring(fetch(url))
    version = max(
        (
            v.text
            for v in metadata.findall("./versioning/versions/version")
            if STABLE.fullmatch(v.text)
        ),
        key=lambda v: tuple(map(int, v.split("."))),
    )
    base = f"https://repo.maven.apache.org/maven2/org/xmtp/android/{version}/android-{version}"
    pom = fetch(base + ".pom")
    packages["kotlin"] = {
        "name": "org.xmtp:android",
        "version": version,
        "metadata_url": url,
        "pom_url": base + ".pom",
        "pom_sha256": hashlib.sha256(pom).hexdigest(),
        "archive_url": base + ".aar",
        "archive_sha256": fetch(base + ".aar.sha256").decode().strip(),
    }
    refs = github("git/matching-refs/tags/ios-")
    ref = max(
        (r for r in refs if STABLE.fullmatch(r["ref"].split("ios-")[-1])),
        key=lambda r: tuple(map(int, r["ref"].split("ios-")[-1].split("."))),
    )
    tag = ref["ref"].removeprefix("refs/tags/")
    release = github("releases/tags/" + tag)
    if release["draft"] or release["prerelease"]:
        raise ValueError("The selected mobile tag is not a published stable release")
    commit = ref["object"]["sha"]
    if ref["object"]["type"] == "tag":
        commit = github("git/tags/" + commit)["object"]["sha"]
    source_url = (
        f"https://raw.githubusercontent.com/xmtp/libxmtp/{commit}/Package.swift"
    )
    source = fetch(source_url)
    binary_urls = re.findall(
        r'https://github.com/xmtp/libxmtp/releases/download/[^"\s]+', source.decode()
    )
    checksums = re.findall(r'checksum:\s*"([a-f0-9]{64})"', source.decode())
    if not binary_urls or len(binary_urls) != len(checksums):
        raise ValueError("Review mobile package binary declarations before pinning")
    packages["swift"] = {
        "name": "XMTPiOS",
        "version": tag.removeprefix("ios-"),
        "tag": tag,
        "commit": commit,
        "published": release["published_at"],
        "metadata_url": release["url"],
        "manifest_url": source_url,
        "manifest_sha256": hashlib.sha256(source).hexdigest(),
        "binary_archives": [
            dict(url=url, sha256=sha) for url, sha in zip(binary_urls, checksums)
        ],
    }
    return {
        "schema": 1,
        "resolved_at": datetime.now(timezone.utc).isoformat(),
        "rule": "Latest stable published package; no dev, nightly, rc, or prerelease",
        "packages": packages,
        "measurement_status": "PENDING: install full dependency closure before size and runtime measurements",
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output")
    args = parser.parse_args()
    Path(args.output).write_text(json.dumps(resolve(), indent=2) + "\n")
