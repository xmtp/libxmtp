#!/usr/bin/env python3
"""Build clean native consumers against one immutable SDK package."""

import argparse
import base64
import hashlib
import importlib.util
import json
import os
import re
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile


HERE = Path(__file__).resolve().parent


def run(command, cwd, **kwargs):
    subprocess.run(command, cwd=cwd, check=True, **kwargs)


def package_verifier():
    spec = importlib.util.spec_from_file_location(
        "rn_package", HERE.parents[1] / "scripts/package.py"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def sdk_identity(package, expected_version, expected_source):
    if not expected_source:
        raise ValueError(
            "pass the exact SDK source identity with --expected-source or XMTP_RN_EXPECTED_SOURCE"
        )
    package_verifier().verify_package(package, expected_version, expected_source)
    with package.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


LEGACY_INPUTS = (
    "react-native-5.7.0.db3",
    "react-native-5.7.0.db3.sqlcipher_salt",
    "expected-records.json",
    "source-inspection.json",
    "unsupported-schema.db3",
    "unsupported-schema.db3.sqlcipher_salt",
    "unsupported-schema.json",
)
IGNORED_INPUTS = {
    "node_modules",
    "build",
    "Pods",
    "DerivedData",
    ".gradle",
    ".cxx",
    "__pycache__",
}


def harness_identity(consumer):
    inputs = {}
    roots = [
        HERE / consumer,
        HERE / "native-hooks",
        HERE.parent / "conformance/type-tests",
    ]
    for folder in roots:
        for path in folder.rglob("*"):
            relative = path.relative_to(folder)
            if any(part in IGNORED_INPUTS for part in relative.parts):
                continue
            if (
                not path.is_file()
                or path.suffix == ".md"
                or path.name
                in {
                    "verification.json",
                    "debug.keystore",
                    ".xcode.env.local",
                    ".DS_Store",
                }
            ):
                continue
            inputs[path.relative_to(HERE.parent).as_posix()] = path
    for path in [
        HERE / "build.py",
        HERE / "prepare-template.py",
        HERE / "App.tsx",
        *(HERE.parent / "conformance").glob("*.ts"),
    ]:
        inputs[path.relative_to(HERE.parent).as_posix()] = path
    for name in LEGACY_INPUTS:
        path = HERE.parent / "conformance/legacy" / name
        inputs[path.relative_to(HERE.parent).as_posix()] = path
    logging_contract = HERE.parents[2] / "browser/test/platform/logging-contract.ts"
    inputs["shared/browser/logging-contract.ts"] = logging_contract
    inputs["shared/node/standardSamples.ts"] = (
        HERE.parents[2] / "node/test/standardSamples.ts"
    )
    hashes = {}
    for name, path in sorted(inputs.items()):
        if path.is_symlink():
            raise ValueError(f"harness source contains symbolic link: {name}")
        hashes[name] = hashlib.sha256(path.read_bytes()).hexdigest()
    return hashlib.sha256(
        json.dumps(hashes, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()


def install_package(work, package, offline):
    """Add only the verified local SDK to the otherwise frozen external graph."""
    with tarfile.open(package) as archive:
        sdk = json.load(archive.extractfile("package/package.json"))
    if sdk.get("dependencies") or sdk.get("optionalDependencies"):
        raise ValueError("offline SDK installation requires its vendored runtime")
    manifest_file = work / "package.json"
    lock_file = work / "pnpm-lock.yaml"
    manifest = json.loads(manifest_file.read_text())
    original = lock_file.read_text()
    peers = sdk.get("peerDependencies", {})
    if set(peers) != {"react", "react-native"}:
        raise ValueError("SDK peers differ from the frozen native fixture")
    versions = {}
    for name, wanted in peers.items():
        if manifest["dependencies"].get(name) != wanted:
            raise ValueError(f"SDK peer {name} differs from the frozen fixture")
        match = re.search(
            r"^      "
            + re.escape(name)
            + r":\n        specifier: "
            + re.escape(wanted)
            + r"\n        version: ([^\n]+)$",
            original,
            re.MULTILINE,
        )
        if not match:
            raise ValueError(f"frozen fixture has no exact {name} peer context")
        versions[name] = match[1]
    # pnpm's real local-tarball lock schema preserves the absolute manifest
    # specifier and records its relative file URL plus the existing peer graph.
    local = "file:" + os.path.relpath(package, work)
    specifier = "file:" + str(package)
    key = sdk["name"] + "@" + local
    context = "".join(
        "(" + peer + ")"
        for peer in sorted(name + "@" + version for name, version in versions.items())
    )
    with package.open("rb") as stream:
        integrity = (
            "sha512-"
            + base64.b64encode(hashlib.file_digest(stream, "sha512").digest()).decode()
        )
    quote = json.dumps
    importer = (
        f"      {quote(sdk['name'])}:\n"
        f"        specifier: {quote(specifier)}\n"
        f"        version: {quote(local + context)}\n"
    )
    node = (
        f"  {quote(key)}:\n"
        + "    resolution: "
        + quote({"integrity": integrity, "tarball": local})
        + "\n"
        + "    version: "
        + quote(sdk["version"])
        + "\n"
        + "    engines: "
        + quote(sdk.get("engines", {}))
        + "\n"
        + "    peerDependencies: "
        + quote(peers)
        + "\n\n"
    )
    snapshot = (
        f"  {quote(key + context)}:\n    dependencies:\n"
        + "".join(
            f"      {quote(name)}: {quote(version)}\n"
            for name, version in versions.items()
        )
        + "\n"
    )
    augmented = original
    for anchor, insertion in (
        ("  .:\n    dependencies:\n", importer),
        ("\npackages:\n\n", node),
        ("\nsnapshots:\n\n", snapshot),
    ):
        if original.count(anchor) != 1:
            raise ValueError("unrecognized frozen fixture lock structure")
        augmented = augmented.replace(anchor, anchor + insertion, 1)
    if sdk["name"] in manifest["dependencies"]:
        raise ValueError("clean fixture already declares the SDK")
    manifest["dependencies"][sdk["name"]] = specifier
    manifest_file.write_text(json.dumps(manifest, indent=2) + "\n")
    lock_file.write_text(augmented)
    command = ["pnpm", "install", "--frozen-lockfile", "--ignore-scripts"]
    if offline:
        command.append("--offline")
    if os.environ.get("PNPM_STORE_DIR"):
        command.extend(["--store-dir", os.environ["PNPM_STORE_DIR"]])
    run(command, work)
    if lock_file.read_text() != augmented:
        raise ValueError("installation changed the frozen external dependency graph")


def android_toolchain_identity():
    value = os.environ.get("XMTP_RN_ANDROID_TOOLCHAIN")
    if not value:
        raise ValueError("Android builds require the declared Nix Java toolchains")
    identity = json.loads(value)
    if (
        not isinstance(identity, dict)
        or set(identity) != {"schema", "runtime", "compiler", "gradle", "policy"}
        or identity["schema"] != 1
    ):
        raise ValueError("Android toolchain receipt has an unsupported schema")
    for role, major in (("runtime", 21), ("compiler", 17)):
        java = identity[role]
        if not isinstance(java, dict) or set(java) != {"javaVersion", "derivationHash"}:
            raise ValueError("Android Java toolchain identity is incomplete")
        if not isinstance(java["javaVersion"], str) or not re.fullmatch(
            rf"{major}\.\d+\.\d+(?:[+_.-][0-9A-Za-z.-]+)*", java["javaVersion"]
        ):
            raise ValueError(f"Android {role} must use the declared Java {major}")
    gradle = identity["gradle"]
    expected = json.loads((HERE / "native-toolchain.json").read_text())["gradle"][
        "version"
    ]
    if (
        not isinstance(gradle, dict)
        or set(gradle) != {"version", "derivationHash"}
        or gradle["version"] != expected
    ):
        raise ValueError("Android Gradle identity differs from the shared pin")
    for tool in (identity["runtime"], identity["compiler"], gradle):
        if not isinstance(tool["derivationHash"], str) or not re.fullmatch(
            r"[0-9a-f]{64}", tool["derivationHash"]
        ):
            raise ValueError("Android toolchain requires exact derivation hashes")
    if identity["policy"] != {"autoDownload": False, "autoDetect": False} or any(
        value is not False for value in identity["policy"].values()
    ):
        raise ValueError("Android Java toolchains must disable download and detection")
    return identity


def check_types(work):
    installed = work / "node_modules/typescript/bin/tsc"
    override = os.environ.get("XMTP_RN_TSC")
    node = shutil.which("node")
    if not override and not node:
        raise ValueError("consumer type checks require the selected Node executable")
    compiler = [override] if override else [node, str(installed)]
    expected = json.loads((work / "package.json").read_text())["devDependencies"][
        "typescript"
    ]
    try:
        version = subprocess.check_output(
            compiler + ["--version"], cwd=work, text=True
        ).strip()
        if version != "Version " + expected:
            raise ValueError(
                f"consumer compiler version differs: {version}; expected {expected}"
            )
        print(f"TypeScript {expected}: {json.dumps(compiler)}", flush=True)
        run(compiler + ["--project", "harness/type-tests/tsconfig.json"], work)
        run(compiler + ["--noEmit"], work)
    except (OSError, subprocess.CalledProcessError):
        paths = [Path(path) for path in compiler if path]
        details = {
            "command": compiler,
            "cwd": str(work),
            "node": node,
            "tscOnPath": shutil.which("tsc"),
            "executables": [],
        }
        for path in paths:
            row = {
                "path": str(path),
                "resolved": str(path.resolve()),
                "exists": path.exists(),
            }
            if path.is_file():
                with path.open("rb") as stream:
                    prefix = stream.read(16).hex()
                row.update(
                    mode=oct(path.stat().st_mode & 0o777),
                    prefixHex=prefix,
                )
            details["executables"].append(row)
        print(
            "TypeScript launch diagnostics: " + json.dumps(details),
            file=sys.stderr,
            flush=True,
        )
        raise


def prepare(consumer, package, work, offline):
    if work.exists():
        raise ValueError(f"clean consumer work directory already exists: {work}")
    template = os.environ.get("XMTP_RN_TEMPLATE")
    if not template:
        template = subprocess.check_output(
            [
                "nix",
                "build",
                "--no-link",
                "--print-out-paths",
                str(HERE.parents[3]) + "#react-native-consumer-template-" + consumer,
            ],
            cwd=HERE.parents[3],
            text=True,
        ).strip()
    shutil.copytree(Path(template).resolve(strict=True), work)
    work.chmod(work.stat().st_mode | 0o200)
    for path in work.rglob("*"):
        path.chmod(path.stat().st_mode | 0o200)
    shutil.copytree(
        HERE / consumer,
        work,
        dirs_exist_ok=True,
        ignore=shutil.ignore_patterns(
            *IGNORED_INPUTS, ".xcode.env.local", "native.patch"
        ),
    )
    signing = json.loads((work / "template-source.json").read_text())["testSigningKey"]
    if (
        hashlib.sha256((work / signing["file"]).read_bytes()).hexdigest()
        != signing["sha256"]
    ):
        raise ValueError(
            "public test signing key differs from its immutable template input"
        )
    hooks = HERE / "native-hooks"
    android_hooks = work / "android/app/src/main/java/com/xmtp/consumer/fixture"
    android_hooks.mkdir(parents=True)
    for name in ("XmtpRuntimeFixture", "XmtpMigrationFixture"):
        shutil.copy2(hooks / f"{name}.kt", android_hooks / f"{name}.kt")
        shutil.copy2(
            hooks / f"{name}.mm",
            work / "ios" / f"XmtpConsumer{consumer.title()}" / f"{name}.mm",
        )
    for assets in (
        work / "android/app/src/main/assets/xmtp-legacy",
        work / "ios/xmtp-legacy",
    ):
        assets.mkdir(parents=True)
        for name in LEGACY_INPUTS:
            shutil.copy2(HERE.parent / "conformance/legacy" / name, assets / name)
    shutil.copy2(
        hooks / "XmtpBackgroundReceiver.kt", android_hooks / "XmtpBackgroundReceiver.kt"
    )
    harness = work / "harness"
    harness.mkdir()
    for source in (HERE.parent / "conformance").glob("*.ts"):
        shutil.copy2(source, harness / source.name)
    shutil.copy2(HERE / "App.tsx", harness / "App.tsx")
    (harness / "build-identity.ts").write_text(
        "export const installedHarnessSha256 = "
        + json.dumps(harness_identity(consumer))
        + ";\n"
    )
    logging_contract = HERE.parents[2] / "browser/test/platform/logging-contract.ts"
    shutil.copy2(logging_contract, harness / "logging-contract.ts")
    samples = HERE.parents[2] / "node/test/standardSamples.ts"
    (harness / "standardSamples.ts").write_text(
        samples.read_text()
        .replace("@xmtp/node-sdk", "@xmtp/react-native-sdk")
        .replace(
            'new TextEncoder().encode("file")', "new Uint8Array([102, 105, 108, 101])"
        )
    )
    vectors = harness / "content-vectors.ts"
    if vectors.is_file():
        vectors.write_text(
            vectors.read_text().replace(
                "../../../node/test/standardSamples", "./standardSamples"
            )
        )
    diagnostics = harness / "diagnostics.ts"
    if diagnostics.is_file():
        diagnostics.write_text(
            diagnostics.read_text().replace(
                "../../../browser/test/platform/logging-contract", "./logging-contract"
            )
        )
    shutil.copytree(HERE.parent / "conformance/type-tests", harness / "type-tests")
    install_package(work, package, offline)
    check_types(work)
    return work


def stamp_local_sdk_pod(work):
    """Adapt only the admitted SDK's version; keep every external Pod locked."""
    ios = work / "ios"
    sdk = work / "node_modules/@xmtp/react-native-sdk"
    expected_hash = (ios / "sdk-podspec-source.sha256").read_text().strip()
    actual_hash = hashlib.sha256(
        (sdk / "XmtpReactNative.podspec").read_bytes()
    ).hexdigest()
    if actual_hash != expected_hash:
        raise ValueError(
            "SDK Podspec changed; resolve and review the native dependency lock"
        )
    baseline = (ios / "XmtpReactNative.podspec.json").read_text()
    specification = json.loads(baseline)
    current_version = specification["version"]
    requested_version = json.loads((sdk / "package.json").read_text())["version"]
    if current_version == requested_version:
        return
    # Normalize checked JSON with CocoaPods itself. Source formatting must not
    # alter the serialized bytes that CocoaPods uses for the local Pod checksum.
    baseline = subprocess.check_output(
        ["pod", "ipc", "spec", str(ios / "XmtpReactNative.podspec.json")],
        cwd=ios,
        text=True,
    )
    if json.loads(baseline) != specification:
        raise ValueError("CocoaPods changed the checked local specification")
    # CocoaPods hashes its serialized local specification. The checked source
    # above proves a version-only release stamp changes exactly these fields.
    rendered = baseline.replace(
        json.dumps(current_version), json.dumps(requested_version), 1
    ).replace(
        json.dumps("react-native-sdk-" + current_version),
        json.dumps("react-native-sdk-" + requested_version),
        1,
    )
    expected = dict(specification)
    expected["version"] = requested_version
    expected["source"] = {
        **specification["source"],
        "tag": "react-native-sdk-" + requested_version,
    }
    if json.loads(rendered) != expected:
        raise ValueError("unexpected local Podspec version stamp")
    lock = ios / "Podfile.lock"
    contents = lock.read_text()
    old_checksum = hashlib.sha1(baseline.encode()).hexdigest()
    new_checksum = hashlib.sha1(rendered.encode()).hexdigest()
    contents, version_changes = re.subn(
        r"(?m)^(  - XmtpReactNative \()" + re.escape(current_version) + r"(\):)$",
        lambda match: match[1] + requested_version + match[2],
        contents,
    )
    contents, checksum_changes = re.subn(
        r"(?m)^  XmtpReactNative: " + old_checksum + "$",
        "  XmtpReactNative: " + new_checksum,
        contents,
    )
    if (version_changes, checksum_changes) != (1, 1):
        raise ValueError("local SDK Pod lock differs from the reviewed baseline")
    lock.write_text(contents)


def offline_ios_environment(work):
    root = Path(os.environ["XMTP_RN_IOS_DOWNLOADS"]).resolve(strict=True)
    manifest = json.loads((HERE / "ios-downloads.json").read_text())
    for row in manifest["artifacts"]:
        source = root / row["path"]
        with source.open("rb") as stream:
            if hashlib.file_digest(stream, "sha256").hexdigest() != row["sha256"]:
                raise ValueError("native iOS dependency digest mismatch")
        target = work / "ios" / row["path"]
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target)
    env = os.environ.copy()
    env["COCOAPODS_DISABLE_STATS"] = "true"
    env.setdefault("CP_HOME_DIR", str(work / ".cocoapods"))
    env.setdefault("CP_CACHE_DIR", str(work / ".cocoapods-cache"))
    repository = Path(env["CP_HOME_DIR"]) / "repos/trunk"
    repository.mkdir(parents=True, exist_ok=True)
    for name, data in json.loads((HERE / "ios-spec-source.json").read_text())[
        "files"
    ].items():
        (repository / name).write_text(data)
    tools = work / ".offline-tools"
    tools.mkdir()
    replay = tools / "curl"
    shutil.copy2(HERE / "offline-curl.py", replay)
    replay.chmod(0o755)
    # Xcode prepends its Python 3.9 to PATH. Bind this helper to the pinned
    # interpreter running the native builder, which provides file_digest.
    replay.write_text(
        "#!" + sys.executable + "\n" + replay.read_text().split("\n", 1)[1]
    )
    env["PATH"] = str(tools) + os.pathsep + env["PATH"]
    env["XMTP_RN_IOS_DOWNLOAD_MANIFEST"] = str(HERE / "ios-downloads.json")
    return env


def build(args):
    if args.platform == "android":
        gradle = os.environ.get("XMTP_RN_GRADLE")
        if (
            not gradle
            or not Path(gradle).is_absolute()
            or not os.access(gradle, os.X_OK)
        ):
            raise ValueError(
                "Android builds require XMTP_RN_GRADLE from dev/nix-shell --shell react-native"
            )
    android_toolchain = (
        android_toolchain_identity() if args.platform == "android" else None
    )
    if os.environ.get("XMTP_NIX_ENV") != "yes" and not os.environ.get("IN_NIX_SHELL"):
        raise ValueError("run consumer builds through dev/nix-shell")
    if args.platform == "ios" and (
        os.environ.get("XMTP_RN_XCODE_MODE") not in {"local", "canonical"}
        or not os.environ.get("XMTP_RN_XCODE_IDENTITY")
    ):
        raise ValueError(
            "iOS builds require explicit canonical/local Xcode mode and exact selected identity"
        )
    package = args.package.resolve(strict=True)
    digest = sdk_identity(package, args.expected_version, args.expected_source)
    work = prepare(args.consumer, package, args.work_dir.resolve(), args.offline)
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    if args.platform == "android":
        command = [os.environ["XMTP_RN_GRADLE"], "--no-daemon"]
        if args.offline and os.environ.get("XMTP_RN_GRADLE_REPLAY") != "1":
            command.append("--offline")
        flags = json.loads(os.environ.get("XMTP_RN_GRADLE_FLAGS_JSON", "[]"))
        if not isinstance(flags, list) or not all(
            isinstance(flag, str) for flag in flags
        ):
            raise ValueError(
                "XMTP_RN_GRADLE_FLAGS_JSON must contain an array of strings"
            )
        command.extend(flags)
        command.extend(
            [
                "assembleRelease",
                "-PreactNativeArchitectures=armeabi-v7a,arm64-v8a,x86,x86_64",
            ]
        )
        run(command, work / "android")
        app = work / "android/app/build/outputs/apk/release/app-release.apk"
        shutil.copy2(app, output / "app.apk")
        app_path = "app.apk"
        app_id = f"com.xmtp.consumer.{args.consumer}"
    else:
        pod_lock = work / "ios/Podfile.lock"
        command = ["pod", "install", "--no-repo-update"]
        if args.offline:
            if not pod_lock.is_file():
                raise ValueError(
                    "offline iOS build requires a resolved committed Podfile.lock and declared Pod downloads"
                )
            command.append("--deployment")
        stamp_local_sdk_pod(work)
        ios_env = offline_ios_environment(work) if args.offline else os.environ.copy()
        run(command, work / "ios", env=ios_env)
        name = f"XmtpConsumer{args.consumer.title()}"
        sdk = "iphoneos" if args.device else "iphonesimulator"
        destination = (
            "generic/platform=iOS" if args.device else "generic/platform=iOS Simulator"
        )
        run(
            [
                "/usr/bin/xcrun",
                "xcodebuild",
                "-workspace",
                f"{name}.xcworkspace",
                "-scheme",
                name,
                "-configuration",
                "Release",
                "-sdk",
                sdk,
                "-destination",
                destination,
                "ARCHS=arm64",
                "-derivedDataPath",
                str(work / "DerivedData"),
                "CODE_SIGNING_ALLOWED=NO",
                "build",
            ],
            work / "ios",
            env=ios_env,
        )
        app = work / "DerivedData/Build/Products" / f"Release-{sdk}" / f"{name}.app"
        shutil.copytree(app, output / app.name)
        import plistlib

        app_id = plistlib.loads((app / "Info.plist").read_bytes())["CFBundleIdentifier"]
        app_path = app.name
    receipt = {
        "schema": 1,
        "consumer": args.consumer,
        "platform": args.platform,
        "device": args.device,
        "packageSha256": digest,
        "harnessSha256": harness_identity(args.consumer),
        **(
            {
                "xcodeMode": os.environ["XMTP_RN_XCODE_MODE"],
                "xcodeIdentity": os.environ["XMTP_RN_XCODE_IDENTITY"],
            }
            if args.platform == "ios"
            else {}
        ),
        **(
            {"androidToolchain": android_toolchain}
            if args.platform == "android"
            else {}
        ),
        "expectedVersion": args.expected_version,
        "expectedSource": args.expected_source,
        "reactNative": "0.88.0",
        "react": "19.3.0",
        "expo": "58.0.14" if args.consumer == "expo" else None,
        "applicationId": app_id,
        "application": app_path,
    }
    (output / "consumer-build.json").write_text(json.dumps(receipt, indent=2) + "\n")


def parser():
    result = argparse.ArgumentParser(description=__doc__)
    result.add_argument("--platform", choices=["android", "ios"], required=True)
    result.add_argument("--consumer", choices=["bare", "expo"], required=True)
    result.add_argument("--package", type=Path, required=True)
    result.add_argument("--work-dir", type=Path, required=True)
    result.add_argument("--output", type=Path, required=True)
    result.add_argument(
        "--expected-version",
        default=os.environ.get("XMTP_RN_EXPECTED_VERSION", "8.0.0"),
    )
    result.add_argument(
        "--expected-source", default=os.environ.get("XMTP_RN_EXPECTED_SOURCE")
    )
    result.add_argument("--offline", action="store_true")
    result.add_argument(
        "--device",
        action="store_true",
        help="compile the iOS arm64 device app instead of the simulator app",
    )
    return result


if __name__ == "__main__":
    build(parser().parse_args())
