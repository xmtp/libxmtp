#!/usr/bin/env python3
"""Validate and assemble immutable React Native SDK components.

Native compilers never run here. Component receipts bind the binaries to the
rendered JS/runtime; assembly needs both complete platform products.
"""

import argparse
from contextlib import contextmanager
import hashlib
import importlib.util
import json
import os
import plistlib
from pathlib import Path, PurePosixPath
import posixpath
import re
import shutil
import subprocess
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parents[3]
PLATFORMS = json.loads((ROOT / "nix/sdk-platforms.json").read_text())
ANDROID = {row["abi"]: row["rustTarget"] for row in PLATFORMS["android"]}
IOS = {"device": "aarch64-apple-ios", "simulator": "aarch64-apple-ios-sim"}
NAME = "@xmtp/react-native-sdk"
CODEGEN = {
    "name": "XmtpReactNativeSpec",
    "type": "modules",
    "jsSrcsDir": "src/specs",
    "android": {"javaPackageName": "org.xmtp.reactnative"},
    "ios": {"modulesProvider": {"XmtpReactNative": "XmtpReactNative"}},
}
IDENTITY_FIELDS = (
    "source",
    "generator",
    "profile",
    "features",
    "forkRevision",
    "renderContract",
    "runtimeContract",
    "javascriptToolchain",
    "productContract",
)
spec = importlib.util.spec_from_file_location(
    "sdk_artifacts", ROOT / "crates/xmtp_sdk/dev/sdk-artifacts.py"
)
artifacts = importlib.util.module_from_spec(spec)
spec.loader.exec_module(artifacts)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def read(path):
    return json.loads(path.read_text())


def inventory(directory):
    result = {}
    for path in sorted(directory.rglob("*")):
        if path.is_symlink():
            raise ValueError(f"package input contains a symbolic link: {path}")
        if path.is_file() and path != directory / "sdk-contract.json":
            result[path.relative_to(directory).as_posix()] = digest(path)
    return result


def record(directory, kind, identity, **extra):
    receipt = {
        "schema": 1,
        "kind": kind,
        "identity": identity,
        **extra,
        "assets": inventory(directory),
    }
    (directory / "sdk-contract.json").write_text(json.dumps(receipt, indent=2) + "\n")
    return receipt


def validate(directory, kind):
    receipt = read(directory / "sdk-contract.json")
    if receipt.get("schema") != 1 or receipt.get("kind") != kind:
        raise ValueError(f"invalid {kind} receipt")
    identity = receipt.get("identity", {})
    if any(field not in identity for field in IDENTITY_FIELDS):
        raise ValueError(f"incomplete {kind} identity")
    if identity["profile"] != "release" or identity["features"] != "":
        raise ValueError("RN products require release default-feature libraries")
    if identity["javascriptToolchain"] != "typescript-7.0.2-es2020-esnext-define-v1":
        raise ValueError("RN JavaScript compiler identity mismatch")
    if identity["forkRevision"] != artifacts.fork_revision():
        raise ValueError("RN runtime fork revision mismatch")
    actual = inventory(directory)
    if actual != receipt.get("assets"):
        changed = sorted(set(actual) ^ set(receipt.get("assets", {})))
        changed += sorted(
            name
            for name in actual
            if name in receipt.get("assets", {})
            and actual[name] != receipt["assets"][name]
        )
        raise ValueError(f"{kind} asset mismatch: {', '.join(changed)}")
    if kind == "react-native-js":
        product = hashlib.sha256(
            json.dumps(actual, separators=(",", ":")).encode()
        ).hexdigest()
        if identity["productContract"] != product:
            raise ValueError("RN JS product contract mismatch")
    return receipt


def same_identity(expected, actual):
    if expected != actual:
        fields = sorted(
            key
            for key in set(expected) | set(actual)
            if expected.get(key) != actual.get(key)
        )
        raise ValueError(f"RN component identity mismatch: {', '.join(fields)}")


def binary_path(platform, name):
    if platform == "android":
        return Path("android/src/main/jniLibs") / name / "libxmtp_sdk.so"
    identifier = "ios-arm64" if name == "device" else "ios-arm64-simulator"
    return Path("ios/XmtpSdkFFI.xcframework") / identifier / "libxmtp_sdk.a"


def ios_metadata(mapping):
    return {
        "CFBundlePackageType": "XFWK",
        "XCFrameworkFormatVersion": "1.0",
        "AvailableLibraries": [
            {
                "LibraryIdentifier": "ios-arm64"
                if name == "device"
                else "ios-arm64-simulator",
                "LibraryPath": "libxmtp_sdk.a",
                "SupportedArchitectures": ["arm64"],
                "SupportedPlatform": "ios",
                **(
                    {"SupportedPlatformVariant": "simulator"}
                    if name == "simulator"
                    else {}
                ),
            }
            for name in mapping
        ],
    }


def component(args):
    js = validate(args.js, "react-native-js")
    mapping = ANDROID if args.platform == "android" else IOS
    if args.fast:
        mapping = (
            {"x86_64": ANDROID["x86_64"]}
            if args.platform == "android"
            else {"simulator": IOS["simulator"]}
        )
    expected_toolchains = read(args.toolchain_identity)
    native = {}
    with artifacts.staged_output(args.out.resolve()) as output:
        for name, target in mapping.items():
            # Both ABI-named Android and Rust-triple-named out-links work.
            library = args.libraries / name
            if not library.exists():
                library = args.libraries / target
            receipt = read(library / "native-provenance.json")
            if any(
                receipt.get(key) != value
                for key, value in {
                    "schema": 1,
                    "source": js["identity"]["source"],
                    "target": target,
                    "profile": "release",
                    "features": "",
                }.items()
            ):
                raise ValueError(f"RN native provenance mismatch: {target}")
            if not receipt.get("toolchain") or receipt[
                "toolchain"
            ] != expected_toolchains.get(target):
                raise ValueError(f"RN native toolchain mismatch: {target}")
            source = (
                library
                / "lib"
                / ("libxmtp_sdk.so" if args.platform == "android" else "libxmtp_sdk.a")
            )
            destination = output / binary_path(args.platform, name)
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, destination)
            if destination.stat().st_size == 0:
                raise ValueError(f"empty RN native library: {target}")
            native[target] = {**receipt, "sha256": digest(destination)}
        if args.platform == "ios":
            with (output / "ios/XmtpSdkFFI.xcframework/Info.plist").open(
                "wb"
            ) as stream:
                plistlib.dump(ios_metadata(mapping), stream, sort_keys=True)
        record(
            output,
            f"react-native-{args.platform}",
            js["identity"],
            native=native,
            complete=not args.fast,
            toolchains={
                target: expected_toolchains[target] for target in mapping.values()
            },
        )


def validate_platform(directory, platform, identity):
    receipt = validate(directory, f"react-native-{platform}")
    same_identity(identity, receipt["identity"])
    mapping = ANDROID if platform == "android" else IOS
    if receipt.get("complete") is not True or set(receipt.get("native", {})) != set(
        mapping.values()
    ):
        raise ValueError(f"RN {platform} component has incomplete target set")
    expected_assets = {binary_path(platform, name).as_posix() for name in mapping}
    if platform == "ios":
        expected_assets.add("ios/XmtpSdkFFI.xcframework/Info.plist")
        with (directory / "ios/XmtpSdkFFI.xcframework/Info.plist").open("rb") as stream:
            if plistlib.load(stream) != ios_metadata(mapping):
                raise ValueError("RN iOS XCFramework slice metadata mismatch")
    if set(receipt["assets"]) != expected_assets:
        raise ValueError(f"RN {platform} component has unexpected native files")
    if set(receipt.get("toolchains", {})) != set(mapping.values()):
        raise ValueError(f"RN {platform} toolchain inventory mismatch")
    for name, target in mapping.items():
        native = receipt["native"][target]
        if (
            not native.get("toolchain")
            or native["toolchain"] != receipt["toolchains"][target]
        ):
            raise ValueError(f"RN {platform} native toolchain mismatch: {target}")
        if any(
            native.get(key) != value
            for key, value in {
                "schema": 1,
                "source": identity["source"],
                "target": target,
                "profile": "release",
                "features": "",
            }.items()
        ) or native.get("sha256") != digest(directory / binary_path(platform, name)):
            raise ValueError(f"RN {platform} native receipt mismatch: {target}")
    return receipt


def validate_package(directory, version=None):
    receipt = validate(directory, "react-native-package")
    manifest = read(directory / "package.json")
    if manifest.get("name") != NAME or manifest.get("version") != receipt.get(
        "packageVersion"
    ):
        raise ValueError("RN package identity/version mismatch")
    if version and manifest["version"] != version:
        raise ValueError(
            f"RN package version {manifest['version']} differs from {version}"
        )
    if manifest.get("codegenConfig") != CODEGEN:
        raise ValueError("RN package native codegen metadata mismatch")
    required = [
        "src/specs/NativeXmtpReactNative.ts",
        "LICENSE",
        "dist/entry.js",
        "dist/entry.d.ts",
        "dist/runtime/core/package.json",
        "THIRD_PARTY_LICENSES/ubrn-LICENSE",
        "cpp/generated/xmtp_sdk.cpp",
        "cpp/generated/xmtp_sdk.hpp",
        "cpp/generated/xmtp_sdk_ffi.hpp",
        "cpp/XmtpNative.cpp",
        "cpp/XmtpNative.h",
        "cpp/XmtpRuntime.cpp",
        "cpp/XmtpRuntime.h",
        "ios/XmtpReactNative.h",
        "ios/XmtpReactNative.mm",
        "android/build.gradle",
        "android/CMakeLists.txt",
        "android/XmtpJni.cpp",
        "android/src/main/AndroidManifest.xml",
        "android/src/main/java/org/xmtp/reactnative/XmtpReactNativeModule.java",
        "android/src/main/java/org/xmtp/reactnative/XmtpReactNativePackage.java",
        "react-native.config.cjs",
        "XmtpReactNative.podspec",
        "ios/XmtpSdkFFI.xcframework/Info.plist",
    ]
    required += [
        binary_path(platform, name).as_posix()
        for platform, mapping in (("android", ANDROID), ("ios", IOS))
        for name in mapping
    ]
    for name in required:
        if name not in receipt["assets"]:
            raise ValueError(f"RN package missing required asset: {name}")
    if digest(directory / "LICENSE") != digest(ROOT / "LICENSE"):
        raise ValueError("RN package XMTP license mismatch")
    with (directory / "ios/XmtpSdkFFI.xcframework/Info.plist").open("rb") as stream:
        if plistlib.load(stream) != ios_metadata(IOS):
            raise ValueError("RN package iOS XCFramework slice metadata mismatch")
    if not any(name.startswith("cpp/runtime/") for name in receipt["assets"]):
        raise ValueError("RN package omits pinned C++ runtime")
    core = read(directory / "dist/runtime/core/package.json")
    for key in ("module", "types"):
        entry = core.get(key)
        if (
            not isinstance(entry, str)
            or not (directory / "dist/runtime/core" / entry).is_file()
        ):
            raise ValueError(f"RN package omits core runtime {key} entry")
    for export in manifest.get("exports", {}).values():
        for path in export.values() if isinstance(export, dict) else [export]:
            if isinstance(path, str) and not (directory / path).is_file():
                raise ValueError(f"RN package export missing: {path}")
    for platform, mapping in (("android", ANDROID), ("ios", IOS)):
        native = receipt.get("components", {}).get(platform, {})
        if native.get("complete") is not True or set(native.get("native", {})) != set(
            mapping.values()
        ):
            raise ValueError(f"RN package lacks complete {platform} receipt")
        same_identity(receipt["identity"], native.get("identity", {}))
        if set(native.get("toolchains", {})) != set(mapping.values()):
            raise ValueError(f"RN package lacks {platform} toolchain identities")
        for name, target in mapping.items():
            if (
                native["native"][target].get("toolchain")
                != native["toolchains"][target]
            ):
                raise ValueError(f"RN package native toolchain mismatch: {target}")
            if native["native"][target].get("sha256") != digest(
                directory / binary_path(platform, name)
            ):
                raise ValueError(f"RN package native digest mismatch: {target}")
    return receipt


@contextmanager
def unpack(path):
    if path.is_dir():
        yield path / "package" if (path / "package").is_dir() else path
        return
    with tempfile.TemporaryDirectory(prefix="xmtp-rn-verify-") as temporary:
        destination = Path(temporary)
        with tarfile.open(path, "r:gz") as archive:
            names = set()
            for member in archive.getmembers():
                canonical = posixpath.normpath(member.name)
                parts = PurePosixPath(canonical).parts
                if (
                    member.name != canonical
                    or "\\" in member.name
                    or not parts
                    or parts[0] != "package"
                    or ".." in parts
                    or member.issym()
                    or member.islnk()
                    or not (member.isfile() or member.isdir())
                    or canonical in names
                ):
                    raise ValueError(f"unsafe RN tarball member: {member.name}")
                names.add(canonical)
            archive.extractall(destination, filter="data")
        yield destination / "package"


def verify_package(path, version=None, source=None):
    with unpack(path) as directory:
        receipt = validate_package(directory, version)
        if source and receipt["identity"]["source"] != source:
            raise ValueError("RN package source identity mismatch")
        return receipt


def assemble(args):
    js = validate(args.js, "react-native-js")
    android = validate_platform(args.android, "android", js["identity"])
    ios = validate_platform(args.ios, "ios", js["identity"])
    version = args.version or read(args.js / "package.json")["version"]
    if not re.fullmatch(r"\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?", version):
        raise ValueError(f"invalid package version: {version}")
    with artifacts.staged_output(args.out.resolve()) as output:
        product = output / "package"
        shutil.copytree(args.js, product)
        for path in [product, *product.rglob("*")]:
            path.chmod(0o755 if path.is_dir() else 0o644)
        (product / "sdk-contract.json").unlink()
        for platform, directory in (("android", args.android), ("ios", args.ios)):
            shutil.copytree(
                directory / platform, product / platform, dirs_exist_ok=True
            )
        manifest = read(product / "package.json")
        manifest["version"] = version
        (product / "package.json").write_text(json.dumps(manifest, indent=2) + "\n")
        receipt = record(
            product,
            "react-native-package",
            js["identity"],
            packageVersion=version,
            components={"android": android, "ios": ios},
        )
        validate_package(product, version)
        npm = os.environ.get("XMTP_SDK_NPM_CLI")
        command = [os.environ.get("NODE", "node"), npm] if npm else ["npm"]
        packed = json.loads(
            subprocess.check_output(
                command
                + [
                    "pack",
                    str(product),
                    "--pack-destination",
                    str(output),
                    "--ignore-scripts",
                    "--json",
                ],
                cwd=product,
                text=True,
            )
        )
        tarball = output / packed[0]["filename"]
        verify_package(tarball, version)
        (output / "sdk-contract.json").write_text(
            json.dumps(
                {
                    "schema": 1,
                    "kind": "react-native-tarball",
                    "identity": receipt["identity"],
                    "packageVersion": version,
                    "tarball": tarball.name,
                    "sha256": digest(tarball),
                },
                indent=2,
            )
            + "\n"
        )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="action", required=True)
    native = commands.add_parser("component")
    native.add_argument("platform", choices=("android", "ios"))
    native.add_argument("--js", required=True, type=Path)
    native.add_argument("--libraries", required=True, type=Path)
    native.add_argument("--out", required=True, type=Path)
    native.add_argument("--toolchain-identity", required=True, type=Path)
    native.add_argument("--fast", action="store_true")
    assembly = commands.add_parser("assemble")
    for key in ("js", "android", "ios", "out"):
        assembly.add_argument(f"--{key}", required=True, type=Path)
    assembly.add_argument("--version")
    verify = commands.add_parser("verify-package")
    verify.add_argument("path", type=Path)
    verify.add_argument("--version")
    verify.add_argument("--source")
    args = parser.parse_args()
    try:
        if args.action == "component":
            component(args)
        elif args.action == "assemble":
            assemble(args)
        else:
            verify_package(args.path, args.version, args.source)
    except (ValueError, OSError, KeyError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"React Native package error: {error}\n")


if __name__ == "__main__":
    main()
