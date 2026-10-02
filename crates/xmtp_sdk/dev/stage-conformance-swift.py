#!/usr/bin/env python3
"""Stage the public Apple product in a private conformance workspace."""

import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[3]
spec = importlib.util.spec_from_file_location(
    "artifacts", Path(__file__).with_name("sdk-artifacts.py")
)
artifacts = importlib.util.module_from_spec(spec)
spec.loader.exec_module(artifacts)


def main():
    generated = ROOT / "target/sdk-conformance/swift"
    index = json.loads(
        (ROOT / "target/sdk-conformance-artifacts/artifacts.json").read_text()
    )
    native = index["artifacts"]["native"]
    contract = json.loads((generated / "sdk-contract.json").read_text())
    if (
        native["source"] != artifacts.source_hash()
        or native["features"] != "conformance"
        or contract["artifact"]["source"] != native["source"]
        or contract["artifact"]["features"] != "conformance"
        or contract["generator"] != artifacts.source_hash(True)
    ):
        raise ValueError("Swift conformance source or feature contract mismatch")
    artifacts.verify(native)
    for name, checksum in contract["artifact"]["files"].items():
        if native["files"].get(name) != checksum:
            raise ValueError("Swift conformance native pair mismatch")
    for name, checksum in contract["files"].items():
        path = generated / (
            name + ".uninstrumented" if name == "xmtp_sdk.swift" else name
        )
        if artifacts.digest(path) != checksum:
            raise ValueError(f"Swift conformance binding mismatch: {name}")
    with tempfile.TemporaryDirectory() as temporary:
        checked = Path(temporary) / "xmtp_sdk.swift"
        shutil.copy2(generated / "xmtp_sdk.swift.uninstrumented", checked)
        subprocess.run(
            [
                "python3",
                str(ROOT / "crates/xmtp_sdk/conformance/inject_callback_counts.py"),
                "swift",
                str(checked),
            ],
            cwd=ROOT,
            check=True,
        )
        if checked.read_bytes() != (generated / "xmtp_sdk.swift").read_bytes():
            raise ValueError("Swift conformance callback instrumentation mismatch")
    library = next(Path(name) for name in native["files"] if name.endswith(".a"))
    manifest = ROOT / "Package.swift"
    if "sdks/ios/Sources/XmtpSdk" not in manifest.read_text():
        raise ValueError("Swift conformance requires the cutover public Apple package")
    fixture = ROOT / "target/sdk-conformance-apple"
    shutil.rmtree(fixture, ignore_errors=True)
    fixture.mkdir(parents=True)
    shutil.copy2(manifest, fixture / "Package.swift")
    original = ROOT / "crates/xmtp_sdk/conformance/swift"
    conformance = fixture / "crates/xmtp_sdk/conformance/swift"
    conformance.mkdir(parents=True)
    shutil.copy2(original / "Package.swift", conformance)
    shutil.copytree(
        original / "Sources/Conformance", conformance / "Sources/Conformance"
    )
    # The root manifest declares its test target even when only Conformance builds.
    shutil.copytree(
        ROOT / "sdks/ios/Tests/XmtpSdkTests", fixture / "sdks/ios/Tests/XmtpSdkTests"
    )
    sources = fixture / "sdks/ios/Sources/XmtpSdk"
    sources.mkdir(parents=True)
    shutil.copy2(generated / "xmtp_sdk.swift", sources)
    shutil.copytree(generated / "runtime", sources, dirs_exist_ok=True)
    shutil.copy2(ROOT / "sdks/ios/Sources/XmtpSdk/AppleLogSink.swift", sources)
    commands = [
        ("inject_swift_cancellation_gate.py", sources / "xmtp_sdk.swift"),
        ("inject_swift_ready_gate.py", sources / "xmtp_sdk.swift"),
        ("inject_reader_gate.py", "swift", sources / "SDKClient.swift"),
        ("inject_event_start_hook.py", "swift", sources / "events/SDKEvents.swift"),
    ]
    for script, *arguments in commands:
        subprocess.run(
            [
                "python3",
                str(ROOT / "crates/xmtp_sdk/conformance" / script),
                *map(str, arguments),
            ],
            cwd=ROOT,
            check=True,
        )
    headers = fixture / "target/Headers"
    headers.mkdir(parents=True)
    shutil.copy2(generated / "xmtp_sdkFFI.h", headers)
    shutil.copy2(generated / "xmtp_sdkFFI.modulemap", headers / "module.modulemap")
    framework = fixture / "sdks/ios/Artifacts/XmtpSdkFFI.xcframework"
    framework.parent.mkdir(parents=True)
    subprocess.run(
        [
            "xcodebuild",
            "-create-xcframework",
            "-library",
            str(library),
            "-headers",
            str(headers),
            "-output",
            str(framework),
        ],
        cwd=ROOT,
        check=True,
    )
    (fixture / "conformance-inputs.json").write_text(
        json.dumps(
            {
                "scope": "instrumented-host-conformance",
                "native": native,
                "generated": contract,
                "public_manifest_sha256": artifacts.digest(manifest),
            },
            indent=2,
        )
        + "\n"
    )


if __name__ == "__main__":
    main()
