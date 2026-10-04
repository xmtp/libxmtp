#!/usr/bin/env python3
"""Record one built SDK library and its pinned Node runtime addon."""

import argparse
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import tomllib

spec = importlib.util.spec_from_file_location(
    "artifacts", Path(__file__).with_name("sdk-artifacts.py")
)
artifacts = importlib.util.module_from_spec(spec)
spec.loader.exec_module(artifacts)
PLATFORMS = {
    "darwin-arm64": ("aarch64-apple-darwin", "libxmtp_sdk.dylib"),
    "linux-x64-gnu": ("x86_64-unknown-linux-gnu", "libxmtp_sdk.so"),
    "linux-x64-musl": ("x86_64-unknown-linux-musl", "libxmtp_sdk.so"),
    "linux-arm64-gnu": ("aarch64-unknown-linux-gnu", "libxmtp_sdk.so"),
    "linux-arm64-musl": ("aarch64-unknown-linux-musl", "libxmtp_sdk.so"),
    "win32-x64-msvc": ("x86_64-pc-windows-msvc", "xmtp_sdk.dll"),
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("target", choices=PLATFORMS)
    parser.add_argument("--library", type=Path, required=True)
    parser.add_argument("--addon", type=Path, required=True)
    native_identity = parser.add_mutually_exclusive_group(required=True)
    native_identity.add_argument("--native-provenance", type=Path)
    native_identity.add_argument("--native-artifacts", type=Path)
    identity = parser.add_mutually_exclusive_group(required=True)
    identity.add_argument("--runtime-provenance", type=Path)
    identity.add_argument("--runtime-source", type=Path)
    parser.add_argument("--out", type=Path, default=Path("target/sdk-node-platforms"))
    args = parser.parse_args()
    revision = tomllib.loads((artifacts.ROOT / "Cargo.toml").read_text())["workspace"][
        "metadata"
    ]["xmtp-sdk-fork"]["rev"]
    rust_target, library = PLATFORMS[args.target]
    addon = f"uniffi-runtime-napi.{args.target}.node"
    if args.native_provenance:
        native = json.loads(args.native_provenance.read_text())
        if native.get("schema") != 1:
            raise ValueError("invalid Node native provenance schema")
    else:
        native = json.loads(args.native_artifacts.read_text())["artifacts"]["native"]
        artifacts.verify(native)
        if str(args.library.resolve()) not in native["files"]:
            raise ValueError("Node library is not in its build receipt")
    if (
        native["source"] != artifacts.source_hash()
        or native["generator"] != artifacts.source_hash(True)
        or native["target"] != rust_target
        or native["features"]
        or native["profile"] != "release"
    ):
        raise ValueError("Node native library source or build contract mismatch")
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
    else:
        receipt = json.loads(args.runtime_provenance.read_text())
        if (
            receipt.get("schema") != 1
            or receipt.get("target") != rust_target
            or receipt.get("addon") != addon
        ):
            raise ValueError("Node runtime target provenance mismatch")
        actual = receipt["revision"]
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
                    "source": native["source"],
                    "generator": native["generator"],
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
