#!/usr/bin/env python3
"""Build tiny C probes and check the NDK output for every supported ABI."""

import argparse
import importlib.util
import json
from pathlib import Path
import subprocess

spec = importlib.util.spec_from_file_location(
    "mobile", Path(__file__).with_name("mobile-package.py")
)
mobile = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mobile)
EXPECTED = {
    "arm64-v8a": (2, 183),
    "armeabi-v7a": (1, 40),
    "x86_64": (2, 62),
    "x86": (1, 3),
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--out", type=Path, default=Path("target/sdk-android-toolchain-proof")
    )
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    source = args.out.resolve() / "probe.c"
    source.write_text("int xmtp_toolchain_probe(void) { return 42; }\n")
    records = {}
    for abi, triple in mobile.ANDROID.items():
        env = mobile.android_environment(triple)
        key = triple.replace("-", "_")
        cc = env["CC_" + key]
        ar = env["AR_" + key]
        linker = env["CARGO_TARGET_" + key.upper() + "_LINKER"]
        folder = args.out.resolve() / abi
        folder.mkdir(exist_ok=True)
        obj = folder / "probe.o"
        library = folder / "probe.so"
        archive = folder / "probe.a"
        commands = [
            [cc, "-fPIC", "-c", str(source), "-o", str(obj)],
            [ar, "rcs", str(archive), str(obj)],
            [linker, "-shared", str(obj), "-o", str(library)],
        ]
        for command in commands:
            print("NDK probe:", " ".join(command), flush=True)
            subprocess.run(command, env=env, check=True)
        binary = library.read_bytes()
        if binary[:4] != b"\x7fELF" or binary[5] != 1:
            raise ValueError(f"NDK probe is not little-endian ELF: {abi}")
        actual = (binary[4], int.from_bytes(binary[18:20], "little"))
        if actual != EXPECTED[abi]:
            raise ValueError(f"NDK probe ABI mismatch: {abi}: {actual}")
        records[abi] = {
            "rust_target": triple,
            "elf_class": actual[0],
            "elf_machine": actual[1],
            "compiler": cc,
            "linker": linker,
            "archiver": ar,
            "library_sha256": mobile.artifacts.digest(library),
        }
    (args.out / "receipt.json").write_text(json.dumps(records, indent=2) + "\n")
    print("NDK tiny C probes passed for all four ABIs. These are not SDK libraries.")


if __name__ == "__main__":
    main()
