"""Stage the independent generated mobile packages. Run inside Nix."""

import argparse
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[3]
parser = argparse.ArgumentParser()
parser.add_argument("target", choices=["ios", "android"])
parser.add_argument(
    "--host-only", action="store_true", help="Swift host proof only; not an iOS release"
)
parser.add_argument(
    "--fast", action="store_true", help="Android host emulator ABI only"
)
args = parser.parse_args()


def run(*cmd):
    subprocess.run(cmd, cwd=ROOT, check=True)


def build(name):
    run("nix", "build", "--no-link", ".#" + name)
    return Path(
        subprocess.check_output(
            ["nix", "path-info", ".#" + name], cwd=ROOT, text=True
        ).strip()
    )


output = ROOT / "target/migration-packages" / args.target
shutil.rmtree(output, ignore_errors=True)
shutil.copytree(ROOT / "sdks/migration" / args.target, output)
if args.target == "ios":
    generated = ROOT / "target/migration-generated/swift"
    source = output / "Sources/XmtpMigration"
    source.mkdir(parents=True)
    shutil.copy2(generated / "XmtpMigration.swift", source)
    headers = ROOT / "target/migration-generated/swift/include"
    headers.mkdir(exist_ok=True)
    shutil.copy2(generated / "XmtpMigrationFFI.h", headers)
    shutil.copy2(generated / "XmtpMigrationFFI.modulemap", headers / "module.modulemap")
    if args.host_only:
        libraries = [ROOT / "target/debug/libxmtp_legacy_migration.a"]
    else:
        libraries = [
            build(name) / "lib/libxmtp_legacy_migration.a"
            for name in [
                "xmtp-migration-native",
                "xmtp-migration-ios-device",
                "xmtp-migration-ios-simulator",
            ]
        ]
    command = ["xcodebuild", "-create-xcframework"]
    for library in libraries:
        command += ["-library", str(library), "-headers", str(headers)]
    run(*command, "-output", str(output / "XmtpMigrationFFI.xcframework"))
else:
    if args.host_only:
        parser.error("--host-only is for the Swift host proof")
    generated = ROOT / "target/migration-generated/kotlin"
    shutil.copytree(generated / "uniffi", output / "src/main/kotlin/uniffi")
    libs = build("xmtp-migration-android-libs" + ("-fast" if args.fast else ""))
    shutil.copytree(libs / "jniLibs", output / "src/main/jniLibs", symlinks=False)
    (output / "src/main/AndroidManifest.xml").write_text("<manifest />\n")
print(output)
