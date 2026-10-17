#!/usr/bin/env python3
"""Compile the generated XMTP callback ABI against real Hermes and Rust under ASan."""

import argparse
import os
from pathlib import Path
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--generated-cpp", type=Path, required=True)
    parser.add_argument("--fork", type=Path, required=True)
    parser.add_argument("--hermes-source", type=Path, required=True)
    parser.add_argument("--hermes-build", type=Path, required=True)
    parser.add_argument("--sdk-library", type=Path, required=True)
    args = parser.parse_args()
    if os.environ.get("XMTP_NIX_ENV") != "yes":
        parser.error("run through dev/nix-shell")
    source = Path(__file__).with_name("stale-callback.cpp")
    generated = args.generated_cpp.resolve(strict=True)
    sdk = args.sdk_library.resolve(strict=True)
    hermes = args.hermes_source.resolve(strict=True)
    build = args.hermes_build.resolve(strict=True)
    fork = args.fork.resolve(strict=True)
    with tempfile.TemporaryDirectory(prefix="xmtp-hermes-callback-") as temporary:
        executable = Path(temporary) / "stale-callback"
        command = [
            "/usr/bin/clang++",
            "-target",
            "arm64-apple-macos14",
            "-std=c++20",
            "-fsanitize=address",
            "-g",
            f'-DGENERATED_WRAPPER="{generated}"',
            *[
                f"-I{directory}"
                for directory in [
                    hermes / "API",
                    hermes / "API/jsi",
                    hermes / "public",
                    fork / "cpp/stubs",
                    fork / "cpp/includes",
                ]
            ],
            str(source),
            str(build / "API/hermes/libhermes.dylib"),
            str(build / "jsi/libjsi.dylib"),
            str(sdk),
            *[
                f"-Wl,-rpath,{directory}"
                for directory in [build / "API/hermes", build / "jsi", sdk.parent]
            ],
            "-o",
            str(executable),
        ]
        subprocess.run(command, check=True)
        subprocess.run([str(executable)], check=True)


if __name__ == "__main__":
    main()
