#!/usr/bin/env python3
"""One SDK version authority, frozen release plans, and precompile materialization."""

import argparse
import datetime as dt
import hashlib
import json
import os
import re
import subprocess
import sys
import tomllib
from pathlib import Path

SDK_PACKAGES = ("node", "browser", "agent", "react-native")
TARGET = re.compile(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)")
SHA = re.compile(r"[0-9a-f]{40}")
MANIFESTS = [f"sdks/{sdk}/package.json" for sdk in SDK_PACKAGES] + [
    "apps/cli/package.json"
]
MIRRORS = ["Cargo.toml", "Cargo.lock", "sdks/android/gradle.properties", *MANIFESTS]


def git(root, *args):
    return subprocess.check_output(["git", "-C", str(root), *args], text=True).strip()


def cargo(root):
    return tomllib.loads((root / "Cargo.toml").read_text())


def target(root):
    return cargo(root)["workspace"]["package"]["version"]


def packages(root):
    """Only workspace-inheriting local packages change; third-party locks do not."""
    found = set()
    for parent in ("crates", "apps"):
        for manifest in (root / parent).glob("*/Cargo.toml"):
            package = tomllib.loads(manifest.read_text()).get("package", {})
            if package.get("version") == {"workspace": True}:
                found.add(package["name"])
    return found


def check(root):
    version = target(root)
    errors = []
    for name in MANIFESTS:
        package = json.loads((root / name).read_text())
        if package["version"] != version:
            errors.append(f"{name}: {package['version']} != {version}")
        for dep, value in package.get("dependencies", {}).items():
            if dep in [f"@xmtp/{sdk}-sdk" for sdk in SDK_PACKAGES]:
                if value not in ("workspace:*", "workspace:^", "workspace:~", version):
                    errors.append(f"{name}: {dep} must resolve to {version}")
    android = (root / "sdks/android/gradle.properties").read_text()
    if re.search(r"^version\s*=\s*(.+)$", android, re.M).group(1).strip() != version:
        errors.append("sdks/android/gradle.properties: version differs from Cargo.toml")
    sdk_crate = tomllib.loads((root / "crates/xmtp_sdk/Cargo.toml").read_text())
    if sdk_crate["package"]["version"] != {"workspace": True}:
        errors.append("xmtp_sdk must inherit version.workspace = true")
    inherited = packages(root)
    for package in tomllib.loads((root / "Cargo.lock").read_text())["package"]:
        if (
            package["name"] in inherited
            and not package.get("source")
            and package["version"] != version
        ):
            errors.append(
                f"Cargo.lock: {package['name']} version differs from Cargo.toml"
            )
    if errors:
        raise ValueError("\n".join(errors))
    return version


def versioned_mirrors(root, version, source_sha=None):
    def read(name):
        if source_sha is None:
            return (root / name).read_text()
        return subprocess.check_output(
            ["git", "-C", str(root), "show", f"{source_sha}:{name}"], text=True
        )

    mirrors = {}
    source = read("Cargo.toml")
    source, count = re.subn(
        r'(\[workspace\.package\][\s\S]*?\bversion\s*=\s*)"[^"]+"',
        lambda m: m[1] + json.dumps(version),
        source,
        count=1,
    )
    if count != 1:
        raise ValueError("Missing workspace.package.version")
    mirrors["Cargo.toml"] = source
    for name in MANIFESTS:
        package = json.loads(read(name))
        package["version"] = version
        for dep, value in package.get("dependencies", {}).items():
            if dep in [f"@xmtp/{sdk}-sdk" for sdk in SDK_PACKAGES]:
                if not value.startswith("workspace:"):
                    package["dependencies"][dep] = version
        mirrors[name] = json.dumps(package, indent=2, ensure_ascii=False) + "\n"
    mirrors["sdks/android/gradle.properties"] = re.sub(
        r"^version\s*=.*$",
        "version=" + version,
        read("sdks/android/gradle.properties"),
        flags=re.M,
    )
    inherited = packages(root)

    def update(block):
        package = tomllib.loads(block)["package"][0]
        if package["name"] in inherited and not package.get("source"):
            block = re.sub(
                r'^version = "[^"]+"$',
                "version = " + json.dumps(version),
                block,
                count=1,
                flags=re.M,
            )
        return block

    mirrors["Cargo.lock"] = re.sub(
        r"\[\[package\]\][\s\S]*?(?=\n\[\[package\]\]|\Z)",
        lambda m: update(m[0]),
        read("Cargo.lock"),
    )
    return mirrors


def sync(root, version):
    for name, content in versioned_mirrors(root, version).items():
        (root / name).write_text(content)
    check(root)


def release_version(base, kind, sha, timestamp, rc=None):
    if not TARGET.fullmatch(base) or not SHA.fullmatch(sha):
        raise ValueError("Expected a stable Cargo target and full source SHA")
    if kind == "dev":
        if rc is not None:
            raise ValueError("Dev releases have no RC number")
        parsed = dt.datetime.strptime(timestamp, "%Y%m%d%H%M%S")
        if parsed.strftime("%Y%m%d%H%M%S") != timestamp:
            raise ValueError("Invalid timestamp")
        return f"{base}-pre.{timestamp}.dev.g{sha[:7]}"
    if kind == "rc":
        if type(rc) is not int or rc < 1:
            raise ValueError("RC number must be a positive integer")
        # Do not sort backwards from the already shipped 8.0.0-rc4.
        suffix = (
            (f"rc{rc}" if rc <= 9 else f"rc9.{rc - 9}")
            if base == "8.0.0"
            else f"rc.{rc}"
        )
        return f"{base}-{suffix}"
    if kind != "final" or rc is not None:
        raise ValueError("Final releases have no RC number")
    return base


def validate_record(record):
    if set(record) != {
        "sourceSha",
        "targetVersion",
        "version",
        "kind",
        "timestamp",
        "rcNumber",
    }:
        raise ValueError("Invalid release plan fields")
    expected = release_version(
        record["targetVersion"],
        record["kind"],
        record["sourceSha"],
        record["timestamp"],
        record["rcNumber"],
    )
    if expected != record["version"]:
        raise ValueError("Release version differs from the frozen plan")
    return record


def materialize(root, record, snapshot=False):
    validate_record(record)
    if git(root, "rev-parse", "HEAD") != record["sourceSha"]:
        raise ValueError("Checkout differs from frozen release source")
    if snapshot and git(root, "status", "--porcelain", "--untracked-files=no"):
        raise ValueError("A release snapshot needs a clean source checkout")
    expected = versioned_mirrors(root, record["version"], record["sourceSha"])
    for name in git(root, "diff", "--name-only", "HEAD").splitlines():
        if (
            name not in expected
            or not (root / name).is_file()
            or (root / name).read_text() != expected[name]
        ):
            raise ValueError(f"Release checkout has changed tracked source: {name}")
    if target(root) not in (record["targetVersion"], record["version"]):
        raise ValueError("Cargo target differs from frozen release plan")
    check(root)
    sync(root, record["version"])
    if snapshot and git(root, "diff", "--name-only", "HEAD", "--", *MIRRORS):
        # Node's Windows cache and artifact recorder require committed inputs.
        # Keep the release plan pinned to the original source; this local-only
        # child commit identifies the exact versioned bytes being compiled.
        git(root, "add", "--", *MIRRORS)
        date = git(root, "show", "-s", "--format=%cI", record["sourceSha"])
        subprocess.run(
            [
                "git",
                "-C",
                str(root),
                "-c",
                "user.name=github-actions[bot]",
                "-c",
                "user.email=github-actions[bot]@users.noreply.github.com",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-qm",
                f"build: materialize SDK {record['version']}",
            ],
            env=dict(os.environ, GIT_AUTHOR_DATE=date, GIT_COMMITTER_DATE=date),
            check=True,
        )


def seal(directory, record):
    validate_record(record)
    receipt = directory / "release-product.json"
    files = {
        str(p.relative_to(directory)).replace("\\", "/"): hashlib.sha256(
            p.read_bytes()
        ).hexdigest()
        for p in sorted(directory.rglob("*"))
        if p.is_file() and p != receipt
    }
    if not files:
        raise ValueError("Cannot seal an empty release product")
    receipt.write_text(json.dumps({"release": record, "files": files}, indent=2) + "\n")


def verify(directory, record):
    validate_record(record)
    receipt = directory / "release-product.json"
    product = json.loads(receipt.read_text())
    if product["release"] != record:
        raise ValueError("Product belongs to a different release")
    actual = {
        str(p.relative_to(directory)).replace("\\", "/"): hashlib.sha256(
            p.read_bytes()
        ).hexdigest()
        for p in directory.rglob("*")
        if p.is_file() and p != receipt
    }
    if actual != product["files"]:
        raise ValueError("Release product checksum mismatch")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root", type=Path, default=Path(__file__).resolve().parent.parent
    )
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("check")
    setter = commands.add_parser("set")
    setter.add_argument("version")
    planner = commands.add_parser("plan")
    planner.add_argument("--kind", choices=("dev", "rc", "final"), required=True)
    planner.add_argument("--rc-number", type=int)
    planner.add_argument(
        "--timestamp", default=dt.datetime.now(dt.timezone.utc).strftime("%Y%m%d%H%M%S")
    )
    planner.add_argument("--output", type=Path, required=True)
    for command in ("materialize", "seal", "verify"):
        child = commands.add_parser(command)
        child.add_argument("--plan", type=Path, required=True)
        if command == "materialize":
            child.add_argument("--snapshot", action="store_true")
        if command in ("seal", "verify"):
            child.add_argument("directory", type=Path)
    args = parser.parse_args()
    if args.command == "check":
        print(check(args.root))
    elif args.command == "set":
        if not TARGET.fullmatch(args.version):
            raise ValueError("The maintained target must be a stable version")
        sync(args.root, args.version)
    elif args.command == "plan":
        if git(args.root, "status", "--porcelain", "--untracked-files=no"):
            raise ValueError("Freeze a release from a clean source checkout")
        base = check(args.root)
        sha = git(args.root, "rev-parse", "HEAD")
        version = release_version(base, args.kind, sha, args.timestamp, args.rc_number)
        record = dict(
            sourceSha=sha,
            targetVersion=base,
            version=version,
            kind=args.kind,
            timestamp=args.timestamp,
            rcNumber=args.rc_number,
        )
        validate_record(record)
        args.output.write_text(json.dumps(record, indent=2) + "\n")
        print(version)
    else:
        record = json.loads(args.plan.read_text())
        if args.command == "materialize":
            materialize(args.root, record, snapshot=args.snapshot)
        elif args.command == "seal":
            seal(args.directory, record)
        else:
            verify(args.directory, record)


if __name__ == "__main__":
    try:
        main()
    except (ValueError, KeyError) as error:
        sys.exit(str(error))
