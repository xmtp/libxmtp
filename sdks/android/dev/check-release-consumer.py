#!/usr/bin/env python3
"""Install the retained Android coordinate without including the SDK project."""

import argparse
import hashlib
from pathlib import Path
import shutil
import subprocess
import tempfile
import xml.etree.ElementTree as ET

SDK = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("repository", type=Path)
    parser.add_argument("version")
    parser.add_argument("--build-only", action="store_true")
    args = parser.parse_args()
    repository = args.repository.resolve()
    publication = repository / "org/xmtp/android" / args.version
    files = [
        publication / f"android-{args.version}.{ext}"
        for ext in ("aar", "pom", "module")
    ]
    for artifact in files:
        if not artifact.is_file():
            parser.error(f"Missing retained publication file: {artifact}")

    inventory = ET.parse(SDK / "gradle/verification-metadata.xml")
    namespace = "{https://schema.gradle.org/dependency-verification}"
    component = ET.SubElement(
        inventory.getroot().find(f"{namespace}components"),
        f"{namespace}component",
        {"group": "org.xmtp", "name": "android", "version": args.version},
    )
    for artifact in files:
        entry = ET.SubElement(
            component, f"{namespace}artifact", {"name": artifact.name}
        )
        ET.SubElement(
            entry,
            f"{namespace}sha256",
            {"value": hashlib.sha256(artifact.read_bytes()).hexdigest()},
        )
    ET.register_namespace("", namespace[1:-1])

    for metadata in ("module", "pom"):
        with tempfile.TemporaryDirectory(prefix=f"xmtp-consumer-{metadata}-") as folder:
            consumer = Path(folder)
            shutil.copytree(SDK / "dev/release-consumer", consumer, dirs_exist_ok=True)
            (consumer / "gradle").mkdir()
            for name in ("toolchain.properties", "android-ndk.gradle"):
                shutil.copyfile(SDK / "gradle" / name, consumer / "gradle" / name)
            shutil.copyfile(SDK / "gradle.properties", consumer / "gradle.properties")
            inventory.write(
                consumer / "gradle/verification-metadata.xml",
                encoding="utf-8",
                xml_declaration=True,
            )
            result = subprocess.run(
                [
                    str(SDK / "gradlew"),
                    "-p",
                    str(consumer),
                    f"-PxmtpRepository={repository}",
                    f"-PxmtpVersion={args.version}",
                    f"-PxmtpMetadata={metadata}",
                    "assembleDebug",
                    "assembleDebugAndroidTest",
                    *([] if args.build_only else ["connectedDebugAndroidTest"]),
                    "--dependency-verification=strict",
                    "--no-daemon",
                    "--stacktrace",
                ],
                check=False,
            )
            if result.returncode:
                for report in consumer.glob(
                    "build/outputs/androidTest-results/connected/**/*.xml"
                ):
                    print(
                        f"{metadata} consumer: {report.name}\n{report.read_text()}",
                        flush=True,
                    )
                return result.returncode
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
