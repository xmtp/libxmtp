#!/usr/bin/env python3
"""Stage the mobile SDK packages from the Nix release libraries.

`stage android` assembles the AAR from a generated Kotlin tree and the Nix
JNI library of each ABI (`.#xmtp-sdk-android-<abi>`). `stage ios` copies the
release xcframework and Swift sources (`.#ios-xcframeworks`) into a SwiftPM
package. The SDK benchmarks read these packages. The ABIs and Apple slices
come from `nix/sdk-platforms.json`.
"""

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import zipfile

ROOT = Path(__file__).resolve().parents[3]
PLATFORMS = json.loads((ROOT / "nix/sdk-platforms.json").read_text())
# Android ABI -> Rust target.
ANDROID = {item["abi"]: item["rustTarget"] for item in PLATFORMS["android"]}
# The Apple slices: the macOS host library, then the iOS slices.
IOS = tuple(
    item["rustTarget"]
    for item in PLATFORMS["node"]
    if item["rustTarget"].endswith("-apple-darwin")
) + tuple(PLATFORMS["apple"])
spec = importlib.util.spec_from_file_location(
    "artifacts", Path(__file__).with_name("sdk-artifacts.py")
)
artifacts = importlib.util.module_from_spec(spec)
spec.loader.exec_module(artifacts)
staged_output = artifacts.staged_output


def run(command, **kwargs):
    subprocess.run(command, cwd=ROOT, check=True, **kwargs)


def nix_libraries(target, directory):
    """Build or substitute the Nix release libraries into out-links."""
    attributes = (
        {"ios": "ios-xcframeworks"}
        if target == "ios"
        else {abi: f"xmtp-sdk-android-{abi}" for abi in ANDROID}
    )
    for name, attribute in attributes.items():
        run(["nix", "build", "--out-link", str(directory / name), f".#{attribute}"])


def record(output, contract):
    files = {
        str(path.relative_to(output)): artifacts.digest(path)
        for path in sorted(output.rglob("*"))
        if path.is_file()
    }
    digest = hashlib.sha256(json.dumps(contract, sort_keys=True).encode())
    (output / "sdk-contract.json").write_text(
        json.dumps(
            {"contract": digest.hexdigest(), **contract, "assets": files}, indent=2
        )
        + "\n"
    )


def preflight(generated_dir, libraries):
    """Reject a Kotlin binding or JNI library that does not match before assembly."""
    generated = json.loads((generated_dir / "kotlin/sdk-contract.json").read_text())
    if generated["generator"] != artifacts.render_hash("kotlin"):
        raise ValueError("mobile binding generator mismatch")
    binding = generated["artifact"]
    if binding["source"] != artifacts.source_hash():
        raise ValueError("mobile binding source mismatch")
    if binding["features"]:
        raise ValueError("mobile binding feature mismatch: expected default bindings")
    for name, expected in generated["files"].items():
        if artifacts.digest(generated_dir / "kotlin" / name) != expected:
            raise ValueError(f"generated mobile artifact mismatch: {name}")
    native = {}
    for abi, triple in ANDROID.items():
        item = json.loads((libraries / abi / "native-provenance.json").read_text())
        if (
            item.get("schema") != 1
            or item.get("source") != binding["source"]
            or item.get("features")
            or item.get("profile") != "release"
            or item.get("target") != triple
        ):
            raise ValueError(f"mobile binding contract mismatch: {triple}")
        library = libraries / abi / "lib/libxmtp_sdk.so"
        native[triple] = {**item, "files": {library.name: artifacts.digest(library)}}
    return generated, native


def apple_slice(triple):
    """The xcframework platform, variant, and architecture of a Rust target."""
    arch, _, system = triple.split("-", 2)
    platform = "macos" if system == "darwin" else "ios"
    variant = "simulator" if system.endswith("-sim") else None
    return platform, variant, {"aarch64": "arm64", "x86_64": "x86_64"}[arch]


def ios_slices(framework):
    """The (platform, variant, architecture) slices of an xcframework."""
    with (framework / "Info.plist").open("rb") as plist:
        libraries = plistlib.load(plist)["AvailableLibraries"]
    return {
        (item["SupportedPlatform"], item.get("SupportedPlatformVariant"), arch)
        for item in libraries
        for arch in item["SupportedArchitectures"]
    }


def stage_ios(libraries, output):
    release = libraries / "ios/XmtpSdkFFI"
    framework = release / "XmtpSdkFFI.xcframework"
    expected = {apple_slice(triple) for triple in IOS}
    if ios_slices(framework) != expected:
        raise ValueError(
            f"iOS framework slices {sorted(map(str, ios_slices(framework)))} "
            f"differ from nix/sdk-platforms.json {sorted(map(str, expected))}"
        )
    shutil.copytree(framework, output / "XmtpSdkFFI.xcframework", symlinks=True)
    shutil.copytree(release / "Sources/XmtpSdk", output / "Sources/XmtpSdk")
    for path in output.rglob("*"):
        path.chmod(path.stat().st_mode | 0o200)
    (output / "Package.swift").write_text("""// swift-tools-version: 6.1
import PackageDescription
let package = Package(name: "XmtpSdk", platforms: [.iOS(.v14), .macOS(.v11)],
    products: [.library(name: "XmtpSdk", targets: ["XmtpSdk"])],
    targets: [.binaryTarget(name: "xmtp_sdkFFI", path: "XmtpSdkFFI.xcframework"),
              .target(name: "XmtpSdk", dependencies: ["xmtp_sdkFFI"])],
    swiftLanguageModes: [.v5])
""")
    return {"slices": list(IOS)}


def stage_android(args, output):
    generated, native = preflight(args.generated, args.libraries)
    project = args.sdk_root.resolve()
    for name in (
        "library/gradle.lockfile",
        "buildscript-gradle.lockfile",
        "gradle/verification-metadata.xml",
    ):
        if not (project / name).is_file():
            raise ValueError(f"Android dependency input missing: {name}")
    jni = output / "jniLibs"
    for abi in ANDROID:
        (jni / abi).mkdir(parents=True)
        shutil.copyfile(
            args.libraries / abi / "lib/libxmtp_sdk.so", jni / abi / "libxmtp_sdk.so"
        )
    run(
        [
            str(project / "gradlew"),
            "-p",
            str(project),
            ":library:assembleRelease",
            "--no-daemon",
            "-Pkotlin.compiler.execution.strategy=in-process",
            "--dependency-verification=strict",
        ],
        env=dict(
            os.environ,
            XMTP_SDK_GENERATED_DIR=str(args.generated.resolve()),
            XMTP_SDK_ANDROID_JNI_DIR=str(jni),
        ),
    )
    shutil.copy2(
        project / "library/build/outputs/aar/library-release.aar",
        output / "xmtp-sdk.aar",
    )
    with zipfile.ZipFile(output / "xmtp-sdk.aar") as archive:
        for abi in ANDROID:
            if f"jni/{abi}/libxmtp_sdk.so" not in archive.namelist():
                raise ValueError(f"AAR missing ABI: {abi}")
        if "classes.jar" not in archive.namelist():
            raise ValueError("AAR missing classes.jar")
    return {
        "generator": generated["generator"],
        "binding": generated["files"],
        "native": native,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("stage",))
    parser.add_argument("target", choices=("ios", "android"))
    parser.add_argument(
        "--generated",
        type=Path,
        default=Path(os.environ.get("XMTP_SDK_GENERATED_DIR", "target/sdk-generated")),
        help="Root of the generated Kotlin tree (Android)",
    )
    parser.add_argument(
        "--libraries",
        type=Path,
        help="Library out-links to stage; by default, build the Nix libraries",
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
        help="Android SDK project to assemble",
    )
    args = parser.parse_args()
    if args.libraries is None:
        args.libraries = ROOT / "target/sdk-mobile-nix"
        nix_libraries(args.target, args.libraries)
    with staged_output(args.out.resolve() / args.target) as output:
        contract = (
            stage_ios(args.libraries, output)
            if args.target == "ios"
            else stage_android(args, output)
        )
        record(output, contract)
    triples = IOS if args.target == "ios" else tuple(ANDROID.values())
    print(f"SDK staged {args.target} supported targets: {', '.join(triples)}")


if __name__ == "__main__":
    main()
