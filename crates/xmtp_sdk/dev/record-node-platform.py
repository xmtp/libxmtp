#!/usr/bin/env python3
"""Record one built SDK library and its pinned Node runtime addon."""

import argparse
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess

spec = importlib.util.spec_from_file_location(
    "artifacts", Path(__file__).with_name("sdk-artifacts.py")
)
artifacts = importlib.util.module_from_spec(spec)
spec.loader.exec_module(artifacts)
PLATFORM_LIST = json.loads((artifacts.ROOT / "nix/sdk-platforms.json").read_text())
PLATFORMS = {
    platform["target"]: (platform["rustTarget"], platform["library"])
    for platform in PLATFORM_LIST["node"]
}


def verify_pair_bytes(receipt, name, path, label):
    if not path.is_file() or receipt.get("files") != {name: artifacts.digest(path)}:
        raise ValueError(f"Node {label} bytes mismatch")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("target", choices=PLATFORMS)
    parser.add_argument("--library", type=Path, required=True)
    parser.add_argument("--addon", type=Path, required=True)
    native_identity = parser.add_mutually_exclusive_group(required=True)
    native_identity.add_argument("--native-provenance", type=Path)
    native_identity.add_argument("--native-artifacts", type=Path)
    parser.add_argument("--runtime-provenance", type=Path, required=True)
    # A cargo pair's `sdk-artifacts.py key` output, taken before the build.
    parser.add_argument("--source-key", type=Path)
    parser.add_argument("--runtime-source", type=Path)
    parser.add_argument("--out", type=Path, default=Path("target/sdk-node-platforms"))
    args = parser.parse_args()
    revision = artifacts.fork_revision()
    rust_target, library = PLATFORMS[args.target]
    addon = f"uniffi-runtime-napi.{args.target}.node"
    if args.native_provenance:
        # A Nix pair names the SDK closure identity of its evaluation, and a
        # cargo pair (Windows) names its commit. Staging checks both once,
        # against the release binding and the release commit.
        native = json.loads(args.native_provenance.read_text())
        if native.get("schema") != 1:
            raise ValueError("invalid Node native provenance schema")
        verify_pair_bytes(native, library, args.library, "native library")
        identity = {"source": native["source"]}
    else:
        native = json.loads(args.native_artifacts.read_text())["artifacts"]["native"]
        artifacts.verify(native)
        if str(args.library.resolve()) not in native["files"]:
            raise ValueError("Node library is not in its build receipt")
        if not native["source"].startswith("commit:"):
            raise ValueError("cargo Node library does not name its commit")
        if "+dirty:" in native["source"]:
            raise ValueError("cargo Node library has uncommitted changes")
        # A cache restore can ship these bytes for a later commit with the
        # same SDK inputs, so the receipt names both the building commit and
        # the source key; staging checks the key.
        if not args.source_key:
            raise ValueError("a cargo Node pair needs --source-key")
        key = json.loads(args.source_key.read_text())
        if key["forkRevision"] != revision:
            raise ValueError("Node source key names another runtime fork")
        identity = {
            "commit": native["source"].removeprefix("commit:"),
            "source": key["source"],
            "sourceKey": key["key"],
        }
    if (
        native["target"] != rust_target
        or native["features"]
        or native["profile"] != "release"
    ):
        raise ValueError("Node native library build contract mismatch")
    receipt = json.loads(args.runtime_provenance.read_text())
    if (
        receipt.get("schema") != 1
        or receipt.get("target") != rust_target
        or receipt.get("addon") != addon
    ):
        raise ValueError("Node runtime target provenance mismatch")
    verify_pair_bytes(receipt, addon, args.addon, "runtime addon")
    actual = receipt["revision"]
    if args.runtime_source:
        actual = subprocess.check_output(
            ["git", "-C", str(args.runtime_source), "rev-parse", "HEAD"], text=True
        ).strip()
        subprocess.run(
            [
                "git",
                "-C",
                str(args.runtime_source),
                "diff",
                "--exit-code",
                "HEAD",
                "--",
                "*.rs",
                "*Cargo.toml",
                "*Cargo.lock",
            ],
            check=True,
        )
        if actual != receipt["revision"]:
            raise ValueError("Node runtime source differs from its build receipt")
    if actual != revision:
        raise ValueError("Node runtime differs from the locked generator fork")
    with artifacts.staged_output(args.out.resolve() / args.target) as output:
        for source, name in ((args.library, library), (args.addon, addon)):
            shutil.copy2(source, output / name)
        (output / "sdk-node-platform.json").write_text(
            json.dumps(
                {
                    "schema": 1,
                    "target": args.target,
                    "rustTarget": rust_target,
                    **identity,
                    "features": "",
                    "profile": "release",
                    "runtimeRevision": revision,
                    "files": {
                        name: artifacts.digest(output / name)
                        for name in (library, addon)
                    },
                },
                indent=2,
            )
            + "\n"
        )


if __name__ == "__main__":
    main()
