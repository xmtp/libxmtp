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
        if re.match(r"^8\.", manifest.get("version", "")) and manifest.get("scripts", {}).get("build") == f"bash ../../dev/js/sdk-package {folder}":
            switched.add(sdk)
    properties = (root / "sdks/android/gradle.properties").read_text()
    build = (root / "sdks/android/library/build.gradle").read_text()
    if re.search(r"(?m)^version=8\.", properties) and "XMTP_SDK_GENERATED_DIR" in build:
        switched.add("Kotlin")
    return switched


if __name__ == "__main__":
    import sys
    directories = {"Swift": "ios", "Kotlin": "android", "Node": "node", "Browser": "browser"}
    root = Path(__file__).resolve().parents[2]
    switched = switched_sdks(root)
    for sdk in sorted(switched):
        print(f"sdks/{directories[sdk]}/")
    # Task 15 moves the agent SDK with the generated Node product.
    agent_path = root / "sdks/agent/package.json"
    if "Node" in switched and agent_path.is_file():
        agent = json.loads(agent_path.read_text())
        if (agent.get("name") == "@xmtp/agent-sdk"
                and re.match(r"^8\.", agent.get("version", ""))
                and agent.get("dependencies", {}).get("@xmtp/node-sdk") == "workspace:*"):
            print("sdks/agent/")
