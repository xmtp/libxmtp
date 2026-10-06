"""Build the Release iOS Simulator app and run one request per app launch."""

import json
import plistlib
import shutil
import subprocess
import time
import uuid
from pathlib import Path

from fixtures import digest

HOSTS = Path(__file__).resolve().parent
BUNDLE = "org.xmtp.benchmark"
SOURCES = ["SwiftSupport.swift", "SwiftSdk.swift", "BenchmarkApp.swift"]
# Termination after a failed or finished operation gets its own short deadline.
CLEANUP_SECONDS = 15
# The app keeps the client databases of a run under STATE/<state key>.
STATE = "Library/Application Support/xmtp-benchmark"
# Each call has a request and response directory here, deleted after the call.
TRANSPORT = "Library/Caches/xmtp-benchmark"
SCHEME = """<?xml version="1.0" encoding="UTF-8"?>
<Scheme LastUpgradeVersion="2700" version="1.3"><BuildAction parallelizeBuildables="NO" buildImplicitDependencies="YES"><BuildActionEntries><BuildActionEntry buildForRunning="YES" buildForProfiling="YES" buildForArchiving="YES" buildForAnalyzing="YES" buildForTesting="NO"><BuildableReference BuildableIdentifier="primary" BlueprintIdentifier="A00000000000000000000004" BuildableName="XmtpBenchmark.app" BlueprintName="XmtpBenchmark" ReferencedContainer="container:Benchmark.xcodeproj"/></BuildActionEntry></BuildActionEntries></BuildAction></Scheme>
"""


def project(package):
    """An Xcode project with one app target that links the staged XmtpSdk."""
    objects = []
    files, builds = [], []
    for number, name in enumerate(SOURCES):
        file_id, build_id = f"F{number:023X}", f"B{number:023X}"
        files.append(file_id)
        builds.append(build_id)
        objects += [
            f'{file_id} = {{isa = PBXFileReference; lastKnownFileType = sourcecode.swift; path = "{name}"; sourceTree = "<group>"; }};',
            f"{build_id} = {{isa = PBXBuildFile; fileRef = {file_id}; }};",
        ]

    def ids(values):
        return ", ".join(values) + ","

    # Xcode identifiers are opaque; fixed values keep the rendered project stable.
    objects += [
        'A00000000000000000000001 = {isa = PBXProject; buildConfigurationList = A00000000000000000000002; compatibilityVersion = "Xcode 14.0"; developmentRegion = en; mainGroup = A00000000000000000000003; packageReferences = (A00000000000000000000009,); projectDirPath = ""; targets = (A00000000000000000000004,); };',
        "A00000000000000000000002 = {isa = XCConfigurationList; buildConfigurations = (A00000000000000000000005,); defaultConfigurationIsVisible = 0; defaultConfigurationName = Release; };",
        f'A00000000000000000000003 = {{isa = PBXGroup; children = ({ids(files)} A00000000000000000000006,); sourceTree = "<group>"; }};',
        'A00000000000000000000004 = {isa = PBXNativeTarget; buildConfigurationList = A00000000000000000000002; buildPhases = (A00000000000000000000007, A00000000000000000000008,); buildRules = (); dependencies = (); name = XmtpBenchmark; packageProductDependencies = (A0000000000000000000000A,); productName = XmtpBenchmark; productReference = A00000000000000000000006; productType = "com.apple.product-type.application"; };',
        f'A00000000000000000000005 = {{isa = XCBuildConfiguration; name = Release; buildSettings = {{ARCHS = arm64; CODE_SIGNING_ALLOWED = NO; GENERATE_INFOPLIST_FILE = NO; INFOPLIST_FILE = Info.plist; IPHONEOS_DEPLOYMENT_TARGET = 16.0; PRODUCT_BUNDLE_IDENTIFIER = "{BUNDLE}"; PRODUCT_NAME = XmtpBenchmark; SDKROOT = iphonesimulator; SUPPORTED_PLATFORMS = iphonesimulator; SWIFT_VERSION = 5.0; SWIFT_OPTIMIZATION_LEVEL = "-O"; SWIFT_COMPILATION_MODE = wholemodule; TARGETED_DEVICE_FAMILY = "1,2"; }}; }};',
        "A00000000000000000000006 = {isa = PBXFileReference; explicitFileType = wrapper.application; path = XmtpBenchmark.app; sourceTree = BUILT_PRODUCTS_DIR; };",
        f"A00000000000000000000007 = {{isa = PBXSourcesBuildPhase; buildActionMask = 2147483647; files = ({ids(builds)}); runOnlyForDeploymentPostprocessing = 0; }};",
        "A00000000000000000000008 = {isa = PBXFrameworksBuildPhase; buildActionMask = 2147483647; files = (A0000000000000000000000B,); runOnlyForDeploymentPostprocessing = 0; };",
        f"A00000000000000000000009 = {{isa = XCLocalSwiftPackageReference; relativePath = {json.dumps(str(package))}; }};",
        "A0000000000000000000000A = {isa = XCSwiftPackageProductDependency; package = A00000000000000000000009; productName = XmtpSdk; };",
        "A0000000000000000000000B = {isa = PBXBuildFile; productRef = A0000000000000000000000A; };",
    ]
    return (
        "// !$*UTF8*$!\n{archiveVersion = 1; classes = {}; objectVersion = 56; objects = {\n"
        + "\n".join(objects)
        + "\n}; rootObject = A00000000000000000000001; }\n"
    )


def booted_simulator():
    listing = json.loads(
        subprocess.run(
            ["xcrun", "simctl", "list", "devices", "booted", "--json"],
            capture_output=True,
            text=True,
            check=True,
        ).stdout
    )
    booted = [d["udid"] for group in listing["devices"].values() for d in group]
    if len(booted) != 1:
        raise ValueError(
            f"Boot one iOS Simulator or pass --simulator ({len(booted)} booted)"
        )
    return booted[0]


def build_app(package, output, udid):
    """Render the project, build it for the simulator and return the app path."""
    output.mkdir(parents=True)
    for name in SOURCES[:2]:
        shutil.copyfile(HOSTS / name, output / name)
    for name in ["BenchmarkApp.swift", "Info.plist"]:
        shutil.copyfile(HOSTS / "ios" / name, output / name)
    xcode = output / "Benchmark.xcodeproj"
    xcode.mkdir()
    (xcode / "project.pbxproj").write_text(project(package))
    schemes = xcode / "xcshareddata/xcschemes"
    schemes.mkdir(parents=True)
    (schemes / "XmtpBenchmark.xcscheme").write_text(SCHEME)
    derived = output / "derived"

    def find(name):
        return subprocess.run(
            ["xcrun", "--find", name], capture_output=True, text=True, check=True
        ).stdout.strip()

    argv = [
        "xcodebuild",
        "-project",
        str(xcode),
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
        f"LD={find('clang')}",
        f"LDPLUSPLUS={find('clang++')}",
        "build",
    ]
    with (output / "build.log").open("w") as log:
        result = subprocess.run(argv, stdout=log, stderr=subprocess.STDOUT)
    if result.returncode:
        raise ValueError(f"iOS app build failed. See {output / 'build.log'}")
    app = derived / "Build/Products/Release-iphonesimulator/XmtpBenchmark.app"
    info = plistlib.loads((app / "Info.plist").read_bytes())
    if info["CFBundleIdentifier"] != BUNDLE:
        raise ValueError("Build did not produce the benchmark app")
    return app


def simctl(commands, *args, cleanup=False, deadline=None):
    remaining = CLEANUP_SECONDS if cleanup else max(0.01, deadline - time.monotonic())
    argv = ["xcrun", "simctl", *args]
    result = subprocess.run(argv, text=True, capture_output=True, timeout=remaining)
    commands.append({"argv": argv, "code": result.returncode, "stderr": result.stderr})
    # Terminating an app that is not running is not an error.
    if result.returncode and not (
        cleanup and "found nothing to terminate" in result.stderr
    ):
        raise RuntimeError(f"simctl {args[0]} failed: {result.stderr}")
    return result


def state_key(state_directory):
    return digest({"state_directory": str(state_directory)})


def install(config):
    deadline = time.monotonic() + config["timeout_seconds"]
    simctl(
        [], "install", config["simulator_udid"], config["app_path"], deadline=deadline
    )


def invoke(config, request, log):
    """Start a fresh app process for one request and read its response file."""
    udid = config["simulator_udid"]
    deadline = time.monotonic() + config["timeout_seconds"]
    commands = []
    operation = uuid.uuid4().hex
    transport = None
    try:
        simctl(commands, "terminate", udid, BUNDLE, cleanup=True)
        container = Path(
            simctl(
                commands, "get_app_container", udid, BUNDLE, "data", deadline=deadline
            ).stdout.strip()
        )
        root = Path(request["state_directory"])
        local = container / STATE / state_key(root)
        local.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(root / "fixture.json", local / "fixture.json")
        host = {
            "backend_url": config["backend_url"],
            "signer_url": config["signer_url"],
        }
        (local / "host.json").write_text(json.dumps(host))
        transport = container / TRANSPORT / operation
        transport.mkdir(parents=True)
        envelope = {
            "operation_id": operation,
            "request": request,
            "state_key": state_key(root),
        }
        (transport / "request.json").write_text(json.dumps(envelope))
        simctl(
            commands,
            "launch",
            "--terminate-running-process",
            udid,
            BUNDLE,
            "--benchmark-request",
            str((transport / "request.json").relative_to(container)),
            deadline=deadline,
        )
        outgoing = transport / "response.json"
        while not outgoing.exists():
            if time.monotonic() >= deadline:
                raise TimeoutError("iOS operation exceeded its deadline")
            time.sleep(0.02)
        value = json.loads(outgoing.read_bytes())
        if value.get("operation_id") != operation:
            raise ValueError("iOS response is for a different operation")
        if "error" in value:
            raise RuntimeError(f"iOS app failed: {value['error']}")
        return value["result"]
    finally:
        try:
            simctl(commands, "terminate", udid, BUNDLE, cleanup=True)
        finally:
            log.with_suffix(".simctl.json").write_text(json.dumps(commands, indent=2))
            if transport:
                shutil.rmtree(transport, ignore_errors=True)


def remove_state(config, state_directory):
    """Delete the client databases of one run from the app container."""
    container = simctl(
        [],
        "get_app_container",
        config["simulator_udid"],
        BUNDLE,
        "data",
        cleanup=True,
    ).stdout.strip()
    local = Path(container) / STATE / state_key(state_directory)
    # A run that failed before its first call has nothing to remove.
    if local.exists():
        shutil.rmtree(local)
