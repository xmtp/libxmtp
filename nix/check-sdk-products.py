#!/usr/bin/env python3
"""Build selected SDK products and check their files, receipts, and Android route."""

import hashlib
import importlib.util
import json
import os
from pathlib import Path
import struct
import subprocess

ROOT = Path(__file__).resolve().parents[1]
EXPRESSION = r"""
systems:
let
  p = systems.${builtins.currentSystem};
  abi = if builtins.match "aarch64-.*" builtins.currentSystem != null
    then "arm64-v8a" else "x86_64";
  names = [ "xmtp-sdk-generated-kotlin" "android-sdk-libs-fast" ]
    ++ (if builtins.hasAttr "ios-libs" p then [ "xmtp-sdk-generated-swift" ] else []);
in {
  inherit names abi;
  outputs = builtins.listToAttrs (map (name: { inherit name; value = toString p.${name}; }) names);
  native = toString p.xmtp-sdk-libs;
  bindgen = toString p.xmtp-sdk-bindgen;
  android = toString p.${"xmtp-sdk-android-${abi}"};
  source = toString p.xmtp-sdk-generated-kotlin.src;
}
"""


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def receipt(tree, source, generator, native, bindgen, excluded=()):
    record = json.loads((tree / "sdk-contract.json").read_text())
    files = {}
    for directory, names, children in os.walk(tree, followlinks=True):
        if Path(directory) == tree:
            names[:] = [name for name in names if name not in excluded]
        for name in children:
            path = Path(directory) / name
            if path == tree / "sdk-contract.json":
                continue
            files[str(path.relative_to(tree))] = digest(path)
    if files != record["files"]:
        raise ValueError(f"{tree}: generated file set or bytes differ from receipt")
    expected_artifact = {
        "source": source,
        "generator": generator,
        "features": "",
        "profile": "release",
        "target": "",
        "files": {str(native): digest(native)},
    }
    if record["artifact"] != expected_artifact or record["generator"] != generator:
        raise ValueError(f"{tree}: native artifact or source identity differs")
    contract = hashlib.sha256(
        json.dumps(
            {
                role: {"files": {path.name: digest(path)}, "generator": generator}
                for role, path in (("native", native), ("bindgen", bindgen))
            },
            sort_keys=True,
        ).encode()
    ).hexdigest()
    if record["contract"] != contract:
        raise ValueError(f"{tree}: receipt does not match native and bindgen bytes")
    print(f"{tree}: {len(files)} generated files and contract match", flush=True)
    return record


def require(tree, names):
    for name in names:
        path = tree / name
        valid = path.is_dir() if name in ("runtime", "android") else path.is_file()
        if not valid:
            raise ValueError(f"{tree}: missing generated output {name}")


def check_products(products):
    source_root = Path(products["source"])
    spec = importlib.util.spec_from_file_location(
        "artifacts", source_root / "crates/xmtp_sdk/dev/sdk-artifacts.py"
    )
    artifacts = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(artifacts)
    source = artifacts.source_hash()
    generator = artifacts.source_hash(True)
    native_root = Path(products["native"])
    native = next(
        path
        for path in (native_root / "lib").iterdir()
        if path.suffix in (".dylib", ".so")
    )
    bindgen = Path(products["bindgen"]) / "bin/xmtp-sdk-bindgen"
    kotlin = Path(products["outputs"]["xmtp-sdk-generated-kotlin"]) / "kotlin"
    require(kotlin, ("uniffi/xmtp_sdk/xmtp_sdk.kt", "runtime", "android"))
    kotlin_record = receipt(kotlin, source, generator, native, bindgen)
    if "xmtp-sdk-generated-swift" in products["outputs"]:
        swift = Path(products["outputs"]["xmtp-sdk-generated-swift"]) / "swift"
        require(
            swift,
            ("xmtp_sdk.swift", "xmtp_sdkFFI.h", "xmtp_sdkFFI.modulemap", "runtime"),
        )
        receipt(swift, source, generator, native, bindgen)
    android = Path(products["outputs"]["android-sdk-libs-fast"])
    require(android, ("uniffi/xmtp_sdk/xmtp_sdk.kt", "runtime", "android"))
    android_record = receipt(android, source, generator, native, bindgen, ("jniLibs",))
    if android_record != kotlin_record:
        raise ValueError(
            "Android fast consumer does not use the selected Kotlin receipt"
        )
    abi = products["abi"]
    if {path.name for path in (android / "jniLibs").iterdir()} != {abi}:
        raise ValueError("Android fast consumer has an unexpected ABI set")
    library = android / "jniLibs" / abi / "libxmtp_sdk.so"
    if digest(library) != digest(Path(products["android"]) / "lib/libxmtp_sdk.so"):
        raise ValueError("Android fast library differs from selected native artifact")
    data = library.read_bytes()
    expected_machine = 183 if abi == "arm64-v8a" else 62
    if (
        data[:6] != b"\x7fELF\x02\x01"
        or struct.unpack_from("<H", data, 18)[0] != expected_machine
    ):
        raise ValueError("Android fast library has an unexpected ELF target")
    provenance = json.loads(
        (Path(products["android"]) / "native-provenance.json").read_text()
    )
    if provenance["source"] != source or provenance["generator"] != generator:
        raise ValueError(
            "Android fast native provenance differs from Kotlin source identity"
        )
    print(
        f"Android fast consumer: matched Kotlin source, receipt, and {abi} ELF library",
        flush=True,
    )


def main():
    products = json.loads(
        subprocess.check_output(
            ["nix", "eval", "--impure", "--json", ".#packages", "--apply", EXPRESSION],
            cwd=ROOT,
        )
    )
    print(json.dumps(products, indent=2), flush=True)
    subprocess.run(
        ["nix", "build", "--no-link", *[".#" + name for name in products["names"]]],
        cwd=ROOT,
        check=True,
    )
    check_products(products)


if __name__ == "__main__":
    main()
