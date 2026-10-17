#!/usr/bin/env python3
"""Compile the production native helper against a controlled UniFFI ABI."""

import os
import platform
from pathlib import Path
import subprocess
import tempfile

compiler = (
    "/usr/bin/clang++"
    if platform.system() == "Darwin"
    else os.environ.get("CXX", "c++")
)
host_flags = ["-mmacosx-version-min=14.0"] if platform.system() == "Darwin" else []
root = Path(__file__).resolve().parents[2]
with tempfile.TemporaryDirectory(prefix="xmtp-rn-native-") as temporary:
    binary = Path(temporary) / "native-test"
    subprocess.run(
        [
            compiler,
            *host_flags,
            "-std=c++20",
            "-pthread",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-I",
            str(root / "test/native"),
            "-I",
            str(root / "cpp"),
            str(root / "test/native/native.cpp"),
            str(root / "cpp/XmtpNative.cpp"),
            "-o",
            str(binary),
        ],
        check=True,
    )
    subprocess.run([str(binary)], check=True, timeout=15)
