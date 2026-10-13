#!/usr/bin/env python3
"""Check architectural boundaries that the Swift compiler cannot enforce."""
from pathlib import Path
import re
import sys

root = Path(__file__).resolve().parents[1]
errors = []
for directory in (root / "ExampleApp", root / "Packages/ExampleKit/Sources/ExampleCore"):
    for source in directory.rglob("*.swift"):
        text = source.read_text()
        if re.search(r"^\s*import\s+XmtpSdk\b", text, re.M):
            errors.append(f"{source}: only ExampleSDK may import XmtpSdk")
        if "ExampleCore" in source.parts and re.search(r"^\s*import\s+(SwiftUI|UIKit)\b", text, re.M):
            errors.append(f"{source}: ExampleCore must not import UI frameworks")
        if re.search(r":\s*ButtonStyle\b", text):
            errors.append(f"{source}: use native glass button styles")
        if "Task.detached" in text:
            errors.append(f"{source}: detached work cannot participate in session ownership")
project = (root / "ExampleApp.xcodeproj/project.pbxproj").read_text()
manifest = (root / "Packages/ExampleKit/Package.swift").read_text()
if "XCRemoteSwiftPackageReference" in project or re.search(r"\.package\([^)]*\burl\s*:", manifest):
    errors.append("the app and ExampleKit must use local package references")
base = (root / "Config/Base.xcconfig").read_text()
for setting, value in (("SWIFT_VERSION", "6.0"), ("SWIFT_STRICT_CONCURRENCY", "complete"), ("SWIFT_TREAT_WARNINGS_AS_ERRORS", "YES"), ("IPHONEOS_DEPLOYMENT_TARGET", "26.0")):
    if not re.search(rf"^{setting}\s*=\s*{re.escape(value)}\s*$", base, re.M):
        errors.append(f"Base.xcconfig: {setting} must be {value}")
    for source in (root / "Config").glob("*.xcconfig"):
        if source.name == "Local.xcconfig":
            continue
        for match in re.finditer(rf"^{setting}\s*=\s*(.*?)\s*$", source.read_text(), re.M):
            if match.group(1) != value:
                errors.append(f"{source}: {setting} weakens the build gate")
    for match in re.finditer(rf"\b{setting}\s*=\s*([^;]+);", project):
        if match.group(1).strip('" ') != value:
            errors.append(f"project.pbxproj: {setting} weakens the build gate")
if "swiftLanguageModes: [.v6]" not in manifest or '"-warnings-as-errors"' not in manifest or '"-strict-concurrency=complete"' not in manifest:
    errors.append("ExampleKit must compile in strict Swift 6 with warnings as errors")
if errors:
    print("\n".join(errors), file=sys.stderr)
    sys.exit(1)
print("Example iOS architecture checks passed.")
