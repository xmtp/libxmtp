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
  renders = [ "xmtp-sdk-generated-kotlin" ]
    ++ (if builtins.hasAttr "ios-xcframeworks" p then [ "xmtp-sdk-generated-swift" ] else []);
  names = renders ++ [ "android-sdk-libs-fast" ];
  nativeDrv = builtins.unsafeDiscardStringContext p.xmtp-sdk-libs.drvPath;
in {
  inherit names abi;
  # Every output the check reads. The renders and the fast consumer only
  # build from the host library and the Android ABI library, so those are not
  # in the outputs' runtime closures.
  read = names ++ [ "xmtp-sdk-libs" "xmtp-sdk-android-${abi}" ];
  outputs = builtins.listToAttrs (map (name: { inherit name; value = toString p.${name}; }) names);
  # Whether each render's build reads the selected host library derivation.
  bound = builtins.listToAttrs (map (name: {
    inherit name;
    value = builtins.hasAttr nativeDrv (builtins.getContext p.${name}.rendering.buildPhase);
  }) renders);
  native = toString p.xmtp-sdk-libs;
  android = toString p.${"xmtp-sdk-android-${abi}"};
}
"""


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def receipt(tree, source, generator, library, excluded=()):
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
    # The receipt digests the library bytes that its render read. Another
    # build of the same library derivation can write other bytes, and the
    # cache keeps only the first upload, so check the library by name here
    # and by derivation in `bound`.
    artifact = dict(record["artifact"])
    named = artifact.pop("files")
    expected_artifact = {
        "source": source,
        "generator": generator,
        "features": "",
        "profile": "release",
        "target": "",
    }
    if (
        artifact != expected_artifact
        or list(named) != [library]
        or record["generator"] != generator
    ):
        raise ValueError(f"{tree}: native artifact or source identity differs")
    print(f"{tree}: {len(files)} generated files and identities match", flush=True)
    return record


def require(tree, names):
    for name in names:
        path = tree / name
        valid = path.is_dir() if name in ("runtime", "android") else path.is_file()
        if not valid:
            raise ValueError(f"{tree}: missing generated output {name}")


def check_products(products):
    unbound = sorted(name for name, bound in products["bound"].items() if not bound)
    if unbound:
        raise ValueError(
            f"{', '.join(unbound)} did not render from the selected host library"
        )
    # The products must name the SDK closure identities of this checkout.
    spec = importlib.util.spec_from_file_location(
        "artifacts", ROOT / "crates/xmtp_sdk/dev/sdk-artifacts.py"
    )
    artifacts = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(artifacts)
    source = artifacts.source_hash()
    native_root = Path(products["native"])
    native = next(
        path
        for path in (native_root / "lib").iterdir()
        if path.suffix in (".dylib", ".so")
    ).name
    provenance = json.loads((native_root / "native-provenance.json").read_text())
    if provenance["source"] != source or "generator" in provenance:
        raise ValueError("Host native provenance differs from the SDK source identity")
    kotlin = Path(products["outputs"]["xmtp-sdk-generated-kotlin"]) / "kotlin"
    require(kotlin, ("uniffi/xmtp_sdk/xmtp_sdk.kt", "runtime", "android"))
    kotlin_record = receipt(kotlin, source, artifacts.render_hash("kotlin"), native)
    if "xmtp-sdk-generated-swift" in products["outputs"]:
        swift = Path(products["outputs"]["xmtp-sdk-generated-swift"]) / "swift"
        require(
            swift,
            ("xmtp_sdk.swift", "xmtp_sdkFFI.h", "xmtp_sdkFFI.modulemap", "runtime"),
        )
        receipt(swift, source, artifacts.render_hash("swift"), native)
    android = Path(products["outputs"]["android-sdk-libs-fast"])
    require(android, ("uniffi/xmtp_sdk/xmtp_sdk.kt", "runtime", "android"))
    android_record = receipt(
        android, source, artifacts.render_hash("kotlin"), native, ("jniLibs",)
    )
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
    if provenance["source"] != source or "generator" in provenance:
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
    # Realize every store path the check reads; on CI they substitute from
    # the sdk_android products job.
    subprocess.run(
        ["nix", "build", "--no-link", *[".#" + name for name in products["read"]]],
        cwd=ROOT,
        check=True,
    )
    check_products(products)


if __name__ == "__main__":
    main()
