#!/usr/bin/env python3
"""Check the JNI library in built Android packages."""

import argparse
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import zipfile


SDK_ROOT = Path(__file__).resolve().parent.parent
PACKAGES = [
    SDK_ROOT / "library/build/outputs/aar/library-debug.aar",
    SDK_ROOT / "library/build/outputs/aar/library-release.aar",
]


def check_package(path, readelf):
    with zipfile.ZipFile(path) as package, tempfile.TemporaryDirectory() as directory:
        libraries = [
            member
            for member in package.infolist()
            if member.filename.endswith("/libxmtp_sdk.so")
        ]
        if not libraries:
            raise ValueError(f"{path}: no SDK JNI library")
        for member in libraries:
            library = Path(directory) / "libxmtp_sdk.so"
            with package.open(member) as source, library.open("wb") as destination:
                shutil.copyfileobj(source, destination)
            output = subprocess.check_output(
                [str(readelf), "--section-headers", str(library)], text=True
            )
            sections = set(re.findall(r"^\s*\[\s*\d+\]\s+(\S+)", output, re.MULTILINE))
            debug = sorted(
                name
                for name in sections
                if name == ".symtab" or name.startswith((".debug_", ".zdebug_"))
            )
            identity = f"{path.name}!{member.filename}"
            if debug:
                raise ValueError(f"{identity}: unstripped sections: {', '.join(debug)}")
            missing = {".dynsym", ".dynstr"} - sections
            if missing:
                raise ValueError(
                    f"{identity}: missing dynamic symbol sections: {', '.join(sorted(missing))}"
                )
            print(f"{identity}: JNI stripping check passed")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("packages", nargs="*", type=Path, default=PACKAGES)
    args = parser.parse_args()
    try:
        ndk = Path(os.environ["ANDROID_NDK_HOME"])
        tools = list((ndk / "toolchains/llvm/prebuilt").glob("*/bin/llvm-readelf"))
        if len(tools) != 1:
            raise ValueError(f"{ndk}: expected one NDK llvm-readelf tool")
        for package in args.packages:
            check_package(package, tools[0])
    except (
        KeyError,
        OSError,
        ValueError,
        zipfile.BadZipFile,
        subprocess.CalledProcessError,
    ) as error:
        print(f"error: Android native package check failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
