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
    args = parser.parse_args()
    triples = IOS if args.target == "ios" else tuple(ANDROID.values())
    if args.action == "build":
        for triple in triples:
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
                ]
            )
        return
    language = "swift" if args.target == "ios" else "kotlin"
    generated = json.loads(
        (args.generated / language / "sdk-contract.json").read_text()
    )
    for name, expected in generated["files"].items():
        if artifacts.digest(args.generated / language / name) != expected:
            raise ValueError(f"generated mobile artifact mismatch: {name}")
    native = {}
    for triple in triples:
        item = json.loads((args.artifacts / triple / "artifacts.json").read_text())[
            "artifacts"
        ]["native"]
        artifacts.verify(item)
        if (
            item["source"] != generated["artifact"]["source"]
            or item["features"]
            or item["profile"] != "release"
        ):
            raise ValueError(f"mobile binding contract mismatch: {triple}")
        native[triple] = item
    output = args.out.resolve() / args.target
    if output.exists():
        shutil.rmtree(output)
    output.mkdir(parents=True)
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
        run(
            [
                "sdks/android/gradlew",
                "-p",
                "crates/xmtp_sdk/packaging/android",
                "assembleRelease",
                "--no-daemon",
            ],
            env=env,
        )
        shutil.copy2(
            ROOT
            / "crates/xmtp_sdk/packaging/android/build/outputs/aar/xmtp-sdk-stage-release.aar",
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
