#!/usr/bin/env python3
"""Generate DocC from current verified bindings in a separate SwiftPM view."""

import argparse
import hashlib
import importlib.util
import json
from pathlib import Path, PurePosixPath
import shutil
import subprocess
import sys
import tempfile

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[3]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def inventory(folder):
    return {
        path.relative_to(folder).as_posix(): digest(path)
        for path in sorted(folder.rglob("*"))
        if path.is_file()
    }


def source_identity(root):
    spec = importlib.util.spec_from_file_location(
        "sdk_artifacts", root / "crates/xmtp_sdk/dev/sdk-artifacts.py"
    )
    artifacts = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(artifacts)
    return artifacts.source_hash(), artifacts.source_hash(True)


def verify_generated(root, generated):
    record = json.loads((generated / "sdk-contract.json").read_text())
    source, generator = source_identity(root)
    artifact = record["artifact"]
    if artifact["source"] != source or record["generator"] != generator:
        raise ValueError("Swift documentation bindings do not match current source")
    # Native provenance can retain an earlier generator. Rendered bindings must be current.
    if (
        artifact["profile"] != "release"
        or artifact["features"] != ""
        or artifact["target"] != ""
    ):
        raise ValueError(
            "Swift documentation requires the selected host release product"
        )
    for name, checksum in record["files"].items():
        relative = PurePosixPath(name)
        if relative.is_absolute() or ".." in relative.parts:
            raise ValueError("Swift generated receipt path escapes its product")
        path = generated / name
        if not path.is_file() or digest(path) != checksum:
            raise ValueError(f"Swift generated file differs from its receipt: {name}")
    actual = inventory(generated)
    actual.pop("sdk-contract.json", None)
    # ios swiftBindings adds duplicate module headers outside the raw receipt.
    for name in tuple(actual):
        if name.startswith("include/"):
            expected = (
                "xmtp_sdkFFI.modulemap"
                if name == "include/module.modulemap"
                else Path(name).name
            )
            if name not in (
                "include/module.modulemap",
                "include/xmtp_sdkFFI.h",
            ) or actual[name] != record["files"].get(expected):
                raise ValueError(
                    f"Swift wrapper header differs from generated input: {name}"
                )
            actual.pop(name)
    if actual != record["files"]:
        raise ValueError("Swift generated file set differs from its receipt")
    for name, checksum in artifact["files"].items():
        path = Path(name)
        if not path.is_file() or digest(path) != checksum:
            raise ValueError("Swift source metadata library differs from its receipt")
    for required in ("xmtp_sdk.swift", "runtime"):
        if not (generated / required).exists():
            raise ValueError(f"Swift generated product is missing {required}")
    return record


def checkout_inputs(root):
    selected = {}
    for name in ("Package.swift", "Package.resolved"):
        selected[name] = digest(root / name)
    for name in ("sdks/ios/Sources", "sdks/ios/Tests"):
        selected.update(
            {f"{name}/{path}": value for path, value in inventory(root / name).items()}
        )
    return selected


def resolved_dependencies(root):
    lock = json.loads((root / "Package.resolved").read_text())
    return {"version": lock["version"], "pins": lock["pins"]}


def prepare_view(root, product, view):
    generated = product / "swift"
    record = verify_generated(root, generated)
    framework = product / "XmtpSdkFFI.xcframework"
    if not (framework / "Info.plist").is_file():
        raise ValueError("Swift documentation framework is missing Info.plist")
    for name in ("Package.swift", "Package.resolved"):
        shutil.copy2(root / name, view / name)
    for name in ("Sources", "Tests"):
        destination = view / "sdks/ios" / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copytree(root / "sdks/ios" / name, destination)
    destination = view / "sdks/ios/Sources/XmtpSdk"
    shutil.copy2(generated / "xmtp_sdk.swift", destination / "xmtp_sdk.swift")
    # Replace generated runtime files as the SDK staging route does.
    runtime = destination / "runtime"
    if runtime.exists():
        shutil.rmtree(runtime)
    shutil.copytree(generated / "runtime", runtime)
    artifacts = view / "sdks/ios/Artifacts"
    artifacts.mkdir(parents=True)
    shutil.copytree(framework, artifacts / framework.name)
    return {
        "source": record["artifact"]["source"],
        "generator": record["generator"],
        "nativeGenerator": record["artifact"]["generator"],
        "contract": record["contract"],
        "profile": record["artifact"]["profile"],
        "generatedFiles": record["files"],
        "frameworkFiles": inventory(framework),
        "checkoutInputs": checkout_inputs(root),
    }


def generate(root, product, output, receipt):
    output = output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    before = checkout_inputs(root)
    sdk_before = source_identity(root)
    with tempfile.TemporaryDirectory(prefix="xmtp-swift-docs-") as directory:
        view = Path(directory)
        inputs = prepare_view(root, product, view)
        dependencies = resolved_dependencies(view)
        if before != checkout_inputs(root) or sdk_before != source_identity(root):
            raise ValueError("Swift documentation inputs changed during preparation")
        command = [
            "swift",
            "package",
            "--allow-writing-to-directory",
            str(output),
            "generate-documentation",
            "--target",
            "XmtpSdk",
            "--disable-indexing",
            "--hosting-base-path",
            "reference/swift",
            "--output-path",
            str(output),
        ]
        subprocess.run(command, cwd=view, check=True)
        if before != checkout_inputs(root) or sdk_before != source_identity(root):
            raise ValueError("Swift documentation changed checkout inputs")
        if dependencies != resolved_dependencies(view):
            raise ValueError("Swift documentation changed resolved dependency pins")
        inputs["resolvedViewInputs"] = checkout_inputs(view)
        inputs["command"] = command
        receipt.parent.mkdir(parents=True, exist_ok=True)
        receipt.write_text(json.dumps(inputs, sort_keys=True) + "\n")
    print(f"Swift docs use current {inputs['profile']} contract {inputs['contract']}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--product", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--receipt", type=Path, required=True)
    args = parser.parse_args()
    generate(ROOT, args.product.resolve(), args.output, args.receipt)


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        print(f"Swift documentation failed: {error}", file=sys.stderr)
        sys.exit(1)
