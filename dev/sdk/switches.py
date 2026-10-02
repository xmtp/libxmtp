"""Identify each SDK that uses the approved generated public product."""

import json
from pathlib import Path
import re


def switched_sdks(root: Path) -> set[str]:
    switched = set()
    package = root / "Package.swift"
    if package.is_file():
        source = package.read_text()
        if 'name: "XmtpSdk"' in source and 'path: "sdks/ios/Sources/XmtpSdk"' in source:
            switched.add("Swift")
    for sdk, folder in (("Node", "node"), ("Browser", "browser")):
        manifest = json.loads((root / f"sdks/{folder}/package.json").read_text())
        if (
            re.match(r"^8\.", manifest.get("version", ""))
            and manifest.get("scripts", {}).get("build")
            == f"bash ../../dev/js/sdk-package {folder}"
        ):
            switched.add(sdk)
    properties = (root / "sdks/android/gradle.properties").read_text()
    build = (root / "sdks/android/library/build.gradle").read_text()
    if re.search(r"(?m)^version=8\.", properties) and "XMTP_SDK_GENERATED_DIR" in build:
        switched.add("Kotlin")
    return switched


if __name__ == "__main__":
    directories = {
        "Swift": "ios",
        "Kotlin": "android",
        "Node": "node",
        "Browser": "browser",
    }
    for sdk in sorted(switched_sdks(Path(__file__).resolve().parents[2])):
        print(f"sdks/{directories[sdk]}/")
