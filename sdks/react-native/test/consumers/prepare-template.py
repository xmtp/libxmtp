#!/usr/bin/env python3
"""Reconstruct native scaffolds from the pinned published template archives."""

import argparse
import json
from pathlib import Path
import subprocess
import tarfile


def prepare(consumer, archive, patch, output):
    prefix = "package/template/" if consumer == "bare" else "package/"
    name = "XmtpConsumer" + consumer.title()
    output.mkdir(parents=True, exist_ok=False)
    with tarfile.open(archive) as source:
        for member in source.getmembers():
            if not member.name.startswith(prefix):
                continue
            relative = member.name.removeprefix(prefix)
            path = Path(relative)
            if not path.parts or path.parts[0] not in {"android", "ios"}:
                continue
            # Builds select Nix Gradle. Upstream launchers and user-specific Xcode
            # workspace state are not native build inputs.
            if (
                member.isdir()
                or path.name in {"gradlew", "gradlew.bat"}
                or "wrapper" in path.parts
                or "project.xcworkspace" in path.parts
            ):
                continue
            if not member.isfile() or path.is_absolute() or ".." in path.parts:
                raise ValueError(f"unsupported native template entry: {relative}")
            relative = (
                relative.replace("HelloWorld", name)
                .replace("com/helloworld", "com/xmtp/consumer/" + consumer)
                .replace("debug.keystore", "fixture-signing-key")
            )
            target = output / relative
            if target.name in {"_xcode.env", "gitignore"}:
                target = target.with_name(
                    ".xcode.env" if target.name == "_xcode.env" else ".gitignore"
                )
            data = source.extractfile(member).read()
            try:
                text = (
                    data.decode("utf-8")
                    .replace("HelloWorld", name)
                    .replace("com.helloworld", "com.xmtp.consumer." + consumer)
                    .replace("helloworld", "xmtpconsumer" + consumer)
                    .replace("debug.keystore", "fixture-signing-key")
                )
                if target.suffix == ".json":
                    text = json.dumps(json.loads(text), indent=2) + "\n"
                data = text.encode()
            except UnicodeDecodeError:
                pass
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(data)
            target.chmod(0o755 if member.mode & 0o111 else 0o644)
    subprocess.run(
        ["patch", "--batch", "--fuzz=0", "--forward", "-p1", "-i", str(patch.resolve())],
        cwd=output,
        check=True,
    )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--consumer", choices=["bare", "expo"], required=True)
    parser.add_argument("--archive", type=Path, required=True)
    parser.add_argument("--patch", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    prepare(args.consumer, args.archive, args.patch, args.output)
