#!/usr/bin/env python3
"""Prepare and build the private iOS host against an installed public product."""

import argparse
import hashlib
import json
import plistlib
import shutil
import subprocess
import sys
from pathlib import Path

HOSTS = Path(__file__).resolve().parents[1]
sys.path[:0] = [str(HOSTS), str(HOSTS.parent)]
from fixtures import digest  # noqa: E402 - requires the path setup above
from ios_identity import dependency_identity, tree_identity  # noqa: E402 - requires the path setup above
from packages import inventory  # noqa: E402 - requires the path setup above


def project(package, product, bundle, sources):
    objects = []
    files, builds = [], []
    for number, name in enumerate(sources):
        file_id, build_id = f"F{number:023X}", f"B{number:023X}"
        files.append(file_id)
        builds.append(build_id)
        objects.extend(
            [
                f'{file_id} = {{isa = PBXFileReference; lastKnownFileType = sourcecode.swift; path = "{name}"; sourceTree = "<group>"; }};',
                f"{build_id} = {{isa = PBXBuildFile; fileRef = {file_id}; }};",
            ]
        )

    def ids(values):
        return ", ".join(values) + ","

    # Xcode identifiers are opaque; fixed values keep the rendered project stable.
    objects.extend(
        [
            'A00000000000000000000001 = {isa = PBXProject; buildConfigurationList = A00000000000000000000002; compatibilityVersion = "Xcode 14.0"; developmentRegion = en; mainGroup = A00000000000000000000003; packageReferences = (A00000000000000000000009,); projectDirPath = ""; targets = (A00000000000000000000004,); };',
            "A00000000000000000000002 = {isa = XCConfigurationList; buildConfigurations = (A00000000000000000000005,); defaultConfigurationIsVisible = 0; defaultConfigurationName = Release; };",
            f'A00000000000000000000003 = {{isa = PBXGroup; children = ({ids(files)} A00000000000000000000006,); sourceTree = "<group>"; }};',
            'A00000000000000000000004 = {isa = PBXNativeTarget; buildConfigurationList = A00000000000000000000002; buildPhases = (A00000000000000000000007, A00000000000000000000008,); buildRules = (); dependencies = (); name = XmtpBenchmark; packageProductDependencies = (A0000000000000000000000A,); productName = XmtpBenchmark; productReference = A00000000000000000000006; productType = "com.apple.product-type.application"; };',
            f'A00000000000000000000005 = {{isa = XCBuildConfiguration; name = Release; buildSettings = {{ARCHS = arm64; CODE_SIGNING_ALLOWED = NO; GENERATE_INFOPLIST_FILE = NO; INFOPLIST_FILE = Info.plist; IPHONEOS_DEPLOYMENT_TARGET = 16.0; PRODUCT_BUNDLE_IDENTIFIER = "{bundle}"; PRODUCT_NAME = XmtpBenchmark; SDKROOT = iphonesimulator; SUPPORTED_PLATFORMS = iphonesimulator; SWIFT_VERSION = 5.0; SWIFT_OPTIMIZATION_LEVEL = "-O"; SWIFT_COMPILATION_MODE = wholemodule; DEBUG_INFORMATION_FORMAT = "dwarf-with-dsym"; TARGETED_DEVICE_FAMILY = "1,2"; }}; }};',
            "A00000000000000000000006 = {isa = PBXFileReference; explicitFileType = wrapper.application; path = XmtpBenchmark.app; sourceTree = BUILT_PRODUCTS_DIR; };",
            f"A00000000000000000000007 = {{isa = PBXSourcesBuildPhase; buildActionMask = 2147483647; files = ({ids(builds)}); runOnlyForDeploymentPostprocessing = 0; }};",
            "A00000000000000000000008 = {isa = PBXFrameworksBuildPhase; buildActionMask = 2147483647; files = (A0000000000000000000000B,); runOnlyForDeploymentPostprocessing = 0; };",
            f"A00000000000000000000009 = {{isa = XCLocalSwiftPackageReference; relativePath = {json.dumps(str(package))}; }};",
            f"A0000000000000000000000A = {{isa = XCSwiftPackageProductDependency; package = A00000000000000000000009; productName = {product}; }};",
            "A0000000000000000000000B = {isa = PBXBuildFile; productRef = A0000000000000000000000A; };",
        ]
    )
    return (
        "// !$*UTF8*$!\n{archiveVersion = 1; classes = {}; objectVersion = 56; objects = {\n"
        + "\n".join(objects)
        + "\n}; rootObject = A00000000000000000000001; }\n"
    )


def prepare(config, output):
    output.mkdir(parents=True, exist_ok=False)
    side = config["side"]
    if side not in ("old", "new"):
        raise ValueError("Side must be old or new")
    package = Path(config["package_root"]).resolve(strict=True)
    snapshot = inventory(package, config["assets"], "swift")
    product = "XMTPiOS" if side == "old" else "XmtpSdk"
    bundle = f"org.xmtp.benchmark.{side}"
    sources = [
        "SwiftSupport.swift",
        "SwiftLive.swift",
        "SwiftOld.swift" if side == "old" else "SwiftNew.swift",
    ]
    for name in sources:
        shutil.copyfile(HOSTS / name, output / name)
    for name in ["BenchmarkApp.swift", "Info.plist"]:
        shutil.copyfile(Path(__file__).parent / name, output / name)
    sources.append("BenchmarkApp.swift")
    source_hash = tree_identity(output)
    build_id = digest(
        {
            "side": side,
            "package_sha256": snapshot["sha256"],
            "sources": source_hash,
            "project_generator": hashlib.sha256(
                Path(__file__).read_bytes()
            ).hexdigest(),
        }
    )
    (output / "BenchmarkIdentity.swift").write_text(
        f'import Foundation\nenum BenchmarkIdentity {{\n    static let side = "{side}"\n    static let packageSHA256 = "{snapshot["sha256"]}"\n    static let buildID = "{build_id}"\n}}\n'
    )
    sources.append("BenchmarkIdentity.swift")
    xcode = output / "Benchmark.xcodeproj"
    xcode.mkdir()
    (xcode / "project.pbxproj").write_text(project(package, product, bundle, sources))
    scheme = xcode / "xcshareddata/xcschemes"
    scheme.mkdir(parents=True)
    (
        scheme / "XmtpBenchmark.xcscheme"
    ).write_text("""<?xml version="1.0" encoding="UTF-8"?>
<Scheme LastUpgradeVersion="2700" version="1.3"><BuildAction parallelizeBuildables="NO" buildImplicitDependencies="YES"><BuildActionEntries><BuildActionEntry buildForRunning="YES" buildForProfiling="YES" buildForArchiving="YES" buildForAnalyzing="YES" buildForTesting="NO"><BuildableReference BuildableIdentifier="primary" BlueprintIdentifier="A00000000000000000000004" BuildableName="XmtpBenchmark.app" BlueprintName="XmtpBenchmark" ReferencedContainer="container:Benchmark.xcodeproj"/></BuildActionEntry></BuildActionEntries></BuildAction></Scheme>
""")
    dependency_root = config.get("dependency_root")
    if dependency_root:
        frozen = (package / dependency_root).resolve(strict=True)
        frozen.relative_to(package)
        lock = frozen / "Package.resolved"
        destination = xcode / "project.xcworkspace/xcshareddata/swiftpm"
        destination.mkdir(parents=True)
        shutil.copyfile(lock, destination / "Package.resolved")
    receipt = {
        "dependency_root": dependency_root,
        "side": side,
        "package_sha256": snapshot["sha256"],
        "package_root": str(package),
        "assets": config["assets"],
        "app_build_id": build_id,
        "bundle_id": bundle,
        "product": product,
        "profile": "Release",
        "target": "arm64-apple-ios-simulator",
        "project_sha256": tree_identity(output),
        "prepared_files": {
            str(p.relative_to(output)): hashlib.sha256(p.read_bytes()).hexdigest()
            for p in sorted(output.rglob("*"))
            if p.is_file() and p.name != "Package.resolved"
        },
    }
    (output / "preparation.json").write_text(json.dumps(receipt, indent=2))
    return receipt


def verify_dependencies(receipt, dependencies, resolved):
    pins = json.loads(resolved.read_text())["pins"] if resolved.exists() else []
    if pins:
        if not receipt["dependency_root"]:
            raise ValueError(
                "Freeze resolved dependencies inside package_root, then prepare again with dependency_root"
            )
        frozen = Path(receipt["package_root"]) / receipt["dependency_root"]
        if json.loads((frozen / "Package.resolved").read_text())["pins"] != pins:
            raise ValueError(
                "Resolved Swift dependency pins differ from the frozen closure"
            )
        for name in ("checkouts", "artifacts"):
            if dependency_identity(dependencies / name) != dependency_identity(
                frozen / name
            ):
                raise ValueError("Resolved dependency bytes differ from frozen " + name)

    return pins


def build(output, udid, derived):
    receipt = json.loads((output / "preparation.json").read_text())
    for name, expected in receipt["prepared_files"].items():
        if hashlib.sha256((output / name).read_bytes()).hexdigest() != expected:
            raise ValueError("Prepared app source changed: " + name)
    if (
        inventory(receipt["package_root"], receipt["assets"], "swift")["sha256"]
        != receipt["package_sha256"]
    ):
        raise ValueError("Installed package changed after preparation")
    argv = [
        "xcodebuild",
        "-project",
        str(output / "Benchmark.xcodeproj"),
        "-scheme",
        "XmtpBenchmark",
        "-configuration",
        "Release",
        "-sdk",
        "iphonesimulator",
        "-destination",
        f"platform=iOS Simulator,id={udid}",
        "-derivedDataPath",
        str(derived),
        "ARCHS=arm64",
        "ONLY_ACTIVE_ARCH=YES",
        "CODE_SIGNING_ALLOWED=NO",
        "LD="
        + subprocess.run(
            ["xcrun", "--find", "clang"], capture_output=True, text=True, check=True
        ).stdout.strip(),
        "LDPLUSPLUS="
        + subprocess.run(
            ["xcrun", "--find", "clang++"], capture_output=True, text=True, check=True
        ).stdout.strip(),
        "build",
    ]
    if receipt["dependency_root"]:
        argv.insert(-1, "-onlyUsePackageVersionsFromResolvedFile")
    with (output / "build.log").open("w") as log:
        subprocess.run(argv, stdout=log, stderr=subprocess.STDOUT, check=True)
    if (
        inventory(receipt["package_root"], receipt["assets"], "swift")["sha256"]
        != receipt["package_sha256"]
    ):
        raise ValueError("Build changed the frozen installed package")
    app = derived / "Build/Products/Release-iphonesimulator/XmtpBenchmark.app"
    info = plistlib.loads((app / "Info.plist").read_bytes())
    if (
        info["CFBundleIdentifier"] != receipt["bundle_id"]
        or info["DTPlatformName"] != "iphonesimulator"
    ):
        raise ValueError("Build did not produce the declared simulator app")
    platform = subprocess.run(
        ["xcrun", "vtool", "-show-build", str(app / "XmtpBenchmark")],
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    architectures = subprocess.run(
        ["xcrun", "lipo", "-archs", str(app / "XmtpBenchmark")],
        capture_output=True,
        text=True,
        check=True,
    ).stdout.strip()
    if "IOSSIMULATOR" not in platform or architectures != "arm64":
        raise ValueError("App executable must be arm64 iOS Simulator")
    dependencies = derived / "SourcePackages"
    resolved = (
        output
        / "Benchmark.xcodeproj/project.xcworkspace/xcshareddata/swiftpm/Package.resolved"
    )
    pins = verify_dependencies(receipt, dependencies, resolved)
    receipt.update(
        app_path=str(app),
        app_sha256=tree_identity(app),
        build_command=argv,
        xcode=subprocess.run(
            ["xcodebuild", "-version"], capture_output=True, text=True, check=True
        ).stdout,
        simulator_udid=udid,
        platform=platform,
        architectures=architectures,
        dependency_pins=pins,
        resolved_dependencies_sha256=tree_identity(dependencies)
        if dependencies.exists()
        else None,
    )
    (output / "build-receipt.json").write_text(json.dumps(receipt, indent=2))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    prep = commands.add_parser("prepare")
    prep.add_argument("config", type=Path)
    prep.add_argument("output", type=Path)
    compile = commands.add_parser("build")
    compile.add_argument("output", type=Path)
    compile.add_argument("udid")
    compile.add_argument("derived", type=Path)
    args = parser.parse_args()
    if args.command == "prepare":
        prepare(json.loads(args.config.read_text()), args.output.resolve())
    else:
        build(args.output.resolve(), args.udid, args.derived.resolve())


if __name__ == "__main__":
    main()
