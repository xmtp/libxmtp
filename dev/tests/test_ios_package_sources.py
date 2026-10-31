"""The evaluated Nix release preserves canonical handwritten Swift filenames."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class IosPackageSourcesTest(unittest.TestCase):
    def test_release_copies_the_apple_adapter_once_under_its_canonical_name(self):
        with tempfile.TemporaryDirectory(prefix="ios-package-") as folder:
            root = Path(folder)
            framework = root / "xmtp-sdk-apple-xcframework/XmtpSdkFFI.xcframework"
            framework.mkdir(parents=True)
            (framework / "Info.plist").write_text("fixture framework")
            swift = root / "xmtp-sdk-swift/swift"
            (swift / "runtime").mkdir(parents=True)
            (swift / "xmtp_sdk.swift").write_text("fixture generated binding")
            (swift / "runtime/Fixture.swift").write_text("fixture runtime")
            expression = f"""
              let
                fixture = {json.dumps(str(root))};
                mkDerivation = args: args // {{ outPath = fixture + "/" + args.pname; }};
                package = import (builtins.toPath {json.dumps(str(ROOT / "nix/package/ios.nix"))}) {{
                  lib = {{}};
                  stdenv = {{ inherit mkDerivation; }};
                  pkgs.stdenv.hostPlatform.rust.rustcTarget = "aarch64-apple-darwin";
                  xmtp.iosEnv.xcode.key = "fixture";
                  xmtp.sdkPlatforms.apple = [];
                  sdk = {{}};
                }};
              in package.release.installPhase
            """
            install = json.loads(
                subprocess.check_output(
                    ["nix", "eval", "--impure", "--json", "--expr", expression],
                    text=True,
                )
            )
            output = root / "output"
            subprocess.run(
                ["bash", "-euc", install],
                env=os.environ | {"out": str(output)},
                check=True,
                capture_output=True,
                text=True,
            )
            sources = output / "XmtpSdkFFI/Sources/XmtpSdk"
            adapter = ROOT / "sdks/ios/Sources/XmtpSdk/AppleLogSink.swift"
            checkout = root / "checkout/sdks/ios/Sources/XmtpSdk"
            checkout.mkdir(parents=True)
            shutil.copyfile(adapter, checkout / "AppleLogSink.swift")
            shutil.copytree(sources, checkout, dirs_exist_ok=True)
            copies = sorted(
                path.relative_to(checkout).as_posix()
                for path in checkout.rglob("*.swift")
                if path.read_bytes() == adapter.read_bytes()
            )
            self.assertEqual(copies, ["AppleLogSink.swift"])
            self.assertEqual(
                (sources / "AppleLogSink.swift").read_bytes(), adapter.read_bytes()
            )
            self.assertEqual(
                sorted(
                    path.relative_to(sources).as_posix()
                    for path in sources.rglob("*.swift")
                ),
                ["AppleLogSink.swift", "runtime/Fixture.swift", "xmtp_sdk.swift"],
            )


if __name__ == "__main__":
    unittest.main()
