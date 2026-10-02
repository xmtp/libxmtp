#!/usr/bin/env python3
"""Build and stage the new mobile SDK without changing the old SDK packages."""

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import zipfile

ROOT = Path(__file__).resolve().parents[3]
IOS = ("aarch64-apple-ios", "aarch64-apple-ios-sim")
ANDROID = {
    "arm64-v8a": "aarch64-linux-android",
    "armeabi-v7a": "armv7-linux-androideabi",
    "x86_64": "x86_64-linux-android",
    "x86": "i686-linux-android",
}
spec = importlib.util.spec_from_file_location(
    "artifacts", Path(__file__).with_name("sdk-artifacts.py")
)
artifacts = importlib.util.module_from_spec(spec)
spec.loader.exec_module(artifacts)


def run(command, **kwargs):
    subprocess.run(command, cwd=ROOT, check=True, **kwargs)


def record(output, generated, native):
    files = {
        str(path.relative_to(output)): artifacts.digest(path)
        for path in sorted(output.rglob("*"))
        if path.is_file()
    }
    contract = hashlib.sha256(
        json.dumps(
            {
                "generator": generated["generator"],
                "binding": generated["files"],
                "native": {
                    name: list(item["files"].values()) for name, item in native.items()
                },
            },
            sort_keys=True,
        ).encode()
    ).hexdigest()
    (output / "sdk-contract.json").write_text(
        json.dumps(
            {
                "contract": contract,
                "generator": generated["generator"],
                "native": native,
                "assets": files,
            },
            indent=2,
        )
        + "\n"
    )


def android_environment(triple):
    """Select the API 23 NDK compiler and archive tools for one Rust target."""
    ndk = (
        os.environ.get("ANDROID_NDK_HOME")
        or os.environ.get("ANDROID_NDK_ROOT")
        or os.environ.get("NDK_HOME")
    )
    if not ndk:
        raise ValueError("Android build requires the android Nix shell and its NDK")
    host = (
        "darwin"
        if sys.platform == "darwin"
        else "windows"
        if sys.platform == "win32"
        else "linux"
    )
    candidates = sorted((Path(ndk) / "toolchains/llvm/prebuilt").glob(host + "-*"))
    if len(candidates) != 1:
        raise ValueError("Android NDK has no unique host toolchain")
    tools = candidates[0] / "bin"
    clang_target = (
        "armv7a-linux-androideabi" if triple == "armv7-linux-androideabi" else triple
    )
    suffix = ".cmd" if sys.platform == "win32" else ""
    cc = tools / (clang_target + "23-clang" + suffix)
    cxx = tools / (clang_target + "23-clang++" + suffix)
    ar = tools / ("llvm-ar.exe" if sys.platform == "win32" else "llvm-ar")
    ranlib = tools / ("llvm-ranlib.exe" if sys.platform == "win32" else "llvm-ranlib")
    for tool in (cc, cxx, ar, ranlib):
        if not tool.is_file():
            raise ValueError(f"Android NDK missing target tool: {tool}")
    target = triple.replace("-", "_")
    env = dict(os.environ)
    env["CARGO_TARGET_" + target.upper() + "_LINKER"] = str(cc)
    env["CC_" + target] = str(cc)
    env["CXX_" + target] = str(cxx)
    env["AR_" + target] = str(ar)
    env["RANLIB_" + target] = str(ranlib)
    return env


def preflight(generated_dir, artifact_dir, target):
    """Reject unmatched binding features and receipts before assembly."""
    language = "swift" if target == "ios" else "kotlin"
    generated = json.loads((generated_dir / language / "sdk-contract.json").read_text())
    if generated["generator"] != artifacts.source_hash(True):
        raise ValueError("mobile binding generator mismatch")
    binding = generated["artifact"]
    if binding["features"]:
        raise ValueError("mobile binding feature mismatch: expected default bindings")
    for name, expected in generated["files"].items():
        if artifacts.digest(generated_dir / language / name) != expected:
            raise ValueError(f"generated mobile artifact mismatch: {name}")
    native = {}
    for triple in IOS if target == "ios" else tuple(ANDROID.values()):
        item = json.loads((artifact_dir / triple / "artifacts.json").read_text())[
            "artifacts"
        ]["native"]
        artifacts.verify(item)
        # Native bytes depend on Rust source. The binding generator can change independently.
        if (
            item["source"] != binding["source"]
            or item["features"]
            or item["profile"] != "release"
            or item["target"] != triple
        ):
            raise ValueError(f"mobile binding contract mismatch: {triple}")
        native[triple] = item
    return generated, native


staged_output = artifacts.staged_output


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("build", "stage"))
    parser.add_argument("target", choices=("ios", "android"))
    parser.add_argument(
        "--generated",
        type=Path,
        default=Path(os.environ.get("XMTP_SDK_GENERATED_DIR", "target/sdk-generated")),
    )
    parser.add_argument(
        "--artifacts", type=Path, default=Path("target/sdk-mobile-artifacts")
    )
    parser.add_argument(
        "--out",
        type=Path,
        default=Path(os.environ.get("XMTP_SDK_PACKAGES_DIR", "target/sdk-packages")),
    )
    parser.add_argument(
        "--sdk-root",
        type=Path,
        default=ROOT / "sdks/android",
        help="Android SDK project to assemble; native receipts still use the common source",
    )
    args = parser.parse_args()
    triples = IOS if args.target == "ios" else tuple(ANDROID.values())
    if args.action == "build":
        for triple in triples:
            env = (
                android_environment(triple)
                if args.target == "android"
                else dict(os.environ)
            )
            run(
                [
                    "python3",
                    "crates/xmtp_sdk/dev/sdk-artifacts.py",
                    "build",
                    "--targets",
                    "swift" if args.target == "ios" else "kotlin",
                    "--rust-target",
                    triple,
                    "--profile",
                    "release",
                    "--skip-bindgen",
                    "--artifacts",
                    str(args.artifacts / triple),
                ],
                env=env,
            )
        return
    language = "swift" if args.target == "ios" else "kotlin"
    generated, native = preflight(args.generated, args.artifacts, args.target)
    with staged_output(args.out.resolve() / args.target) as output:
        if args.target == "ios":
            headers = output / "Headers"
            headers.mkdir()
            shutil.copy2(args.generated / language / "xmtp_sdkFFI.h", headers)
            shutil.copy2(
                args.generated / language / "xmtp_sdkFFI.modulemap",
                headers / "module.modulemap",
            )
            command = ["xcodebuild", "-create-xcframework"]
            for triple in triples:
                library = next(
                    name for name in native[triple]["files"] if name.endswith(".a")
                )
                command += ["-library", library, "-headers", str(headers)]
            run(command + ["-output", str(output / "XmtpSdkFFI.xcframework")])
            sources = output / "Sources/XmtpSdk"
            sources.mkdir(parents=True)
            shutil.copy2(args.generated / language / "xmtp_sdk.swift", sources)
            shutil.copytree(
                args.generated / language / "runtime", sources, dirs_exist_ok=True
            )
            (output / "Package.swift").write_text("""// swift-tools-version: 6.1
import PackageDescription
let package = Package(name: "XmtpSdk", platforms: [.iOS(.v14)],
    products: [.library(name: "XmtpSdk", targets: ["XmtpSdk"])],
    targets: [.binaryTarget(name: "xmtp_sdkFFI", path: "XmtpSdkFFI.xcframework"),
              .target(name: "XmtpSdk", dependencies: ["xmtp_sdkFFI"])],
    swiftLanguageModes: [.v5])
""")
            shutil.rmtree(headers)
        else:
            jni = output / "jniLibs"
            for abi, triple in ANDROID.items():
                (jni / abi).mkdir(parents=True)
                library = next(
                    name for name in native[triple]["files"] if name.endswith(".so")
                )
                shutil.copy2(library, jni / abi / "libxmtp_sdk.so")
            env = dict(
                os.environ,
                XMTP_SDK_GENERATED_DIR=str(args.generated.resolve()),
                XMTP_SDK_ANDROID_JNI_DIR=str(jni),
            )
            project = args.sdk_root.resolve()
            for name in (
                "library/gradle.lockfile",
                "buildscript-gradle.lockfile",
                "gradle/verification-metadata.xml",
            ):
                if not (project / name).is_file():
                    raise ValueError(f"Android dependency input missing: {name}")
            run(
                [
                    str(args.sdk_root.resolve() / "gradlew"),
                    "-p",
                    str(args.sdk_root.resolve()),
                    ":library:assembleRelease",
                    "--no-daemon",
                    "-Pkotlin.compiler.execution.strategy=in-process",
                    "--dependency-verification=strict",
                ],
                env=env,
            )
            shutil.copy2(
                args.sdk_root.resolve()
                / "library/build/outputs/aar/library-release.aar",
                output / "xmtp-sdk.aar",
            )
            with zipfile.ZipFile(output / "xmtp-sdk.aar") as archive:
                for abi in ANDROID:
                    if f"jni/{abi}/libxmtp_sdk.so" not in archive.namelist():
                        raise ValueError(f"AAR missing ABI: {abi}")
                if "classes.jar" not in archive.namelist():
                    raise ValueError("AAR missing classes.jar")
        record(output, generated, native)
    print(f"SDK staged {args.target} supported targets: {', '.join(triples)}")


if __name__ == "__main__":
    main()
