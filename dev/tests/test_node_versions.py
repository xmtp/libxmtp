#!/usr/bin/env python3.11
"""Keep Node tooling pins, sources, and Nix version checks consistent."""

import base64
import json
from pathlib import Path
import re
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[2]


class NodeVersionsTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        overlay = ROOT / "nix/lib/node-versions.nix"
        expression = f"""
          let
            package.override = dependencies: {{
              overrideAttrs = update:
                let
                  previousAttrs = {{
                    patches = [
                      "/fix-temporal-integration-with-shared-icu.patch"
                      "/configure-emulator.patch"
                      "/retained.patch"
                    ];
                    passthru.tests.retained = true;
                    meta.description = "Node.js";
                    runtimeDependencies = dependencies.callPackage;
                    openssl = dependencies.openssl;
                  }};
                in previousAttrs // update
                  {{ finalPackage = "updated-node"; }} previousAttrs;
            }};
            previous = {{
              nodejs-slim_26 = package;
              nodejs-slim_24 = package;
              fetchurl = source: source;
              testers.testVersion = args: args;
              newScope = dependencies: dependencies;
              brotli.overrideAttrs = update: update {{ patches = []; }};
              c-ares.overrideAttrs = update: update {{}};
              openssl.overrideAttrs = update: update {{}};
              ngtcp2.override = dependencies: dependencies;
              nghttp2.override = dependencies: dependencies;
            }};
          in import {overlay} {{}} previous
        """
        result = subprocess.run(
            ["nix-instantiate", "--eval", "--strict", "--json", "--expr", expression],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=True,
        )
        cls.packages = json.loads(result.stdout)

    def test_host_pins_match_nix_node_26(self):
        version = self.packages["nodejs-slim_26"]["version"]
        for path in (".node-version", ".nvmrc"):
            with self.subTest(path=path):
                self.assertEqual((ROOT / path).read_text().strip(), version)

    def test_overrides_stay_within_each_existing_major(self):
        for major in ("24", "26"):
            with self.subTest(major=major):
                package = self.packages[f"nodejs-slim_{major}"]
                self.assertEqual(package["version"].split(".")[0], major)

    def test_sources_and_version_checks_follow_the_override(self):
        for name, package in self.packages.items():
            with self.subTest(package=name):
                version = package["version"]
                self.assertEqual(
                    package["src"]["url"],
                    f"https://nodejs.org/dist/v{version}/node-v{version}.tar.xz",
                )
                algorithm, digest = package["src"]["hash"].split("-", 1)
                self.assertEqual(algorithm, "sha256")
                self.assertEqual(len(base64.b64decode(digest, validate=True)), 32)
                checks = package["passthru"]["tests"]
                self.assertTrue(checks["retained"])
                self.assertEqual(checks["version"]["version"], f"v{version}")
                self.assertEqual(checks["version"]["package"], "updated-node")
                self.assertEqual(package["meta"]["description"], "Node.js")

    def test_only_the_obsolete_upstreamed_patches_are_removed(self):
        for name, package in self.packages.items():
            with self.subTest(package=name):
                self.assertEqual(package["patches"], ["/retained.patch"])

    def test_shared_dependencies_receive_the_release_security_fixes(self):
        for name, package in self.packages.items():
            with self.subTest(package=name):
                dependencies = package["runtimeDependencies"]
                patch = dependencies["brotli"]["patches"][0]
                self.assertIn("0ed4268a4401abcb4472096340477cf4af7d1a2b", patch["url"])
                self.assertEqual(dependencies["c-ares"]["version"], "1.34.9")
                self.assertEqual(dependencies["openssl"]["version"], "3.6.5")
                self.assertEqual(package["openssl"], dependencies["openssl"])
                self.assertEqual(
                    dependencies["ngtcp2"]["brotli"], dependencies["brotli"]
                )
                self.assertEqual(
                    dependencies["ngtcp2"]["openssl"], dependencies["openssl"]
                )
                self.assertEqual(
                    dependencies["nghttp2"]["c-aresMinimal"], dependencies["c-ares"]
                )
                self.assertEqual(
                    dependencies["nghttp2"]["openssl"], dependencies["openssl"]
                )

    def test_ci_node_22_pins_match_the_pinned_container_allowlist(self):
        workflow = (ROOT / ".github/workflows/build-sdk-node-platforms.yml").read_text()
        platforms = json.loads((ROOT / "nix/sdk-platforms.json").read_text())
        images = {
            platform["loadImage"]
            for platform in platforms["node"]
            if "loadImage" in platform
        }
        allowed = set(
            re.findall(
                r"node:22\.\d+\.\d+-(?:bookworm-slim|alpine)@sha256:[0-9a-f]{64}",
                workflow,
            )
        )
        self.assertEqual(images, allowed)
        self.assertEqual(len(images), 2)
        for image in images:
            self.assertTrue(image.startswith("node:22.23.4-"))
        self.assertIn('node-version: "22.23.4"', workflow)
        self.assertNotIn("node:22.12.0", workflow)

    def test_sdk_consumer_minimums_do_not_require_the_security_patch(self):
        for sdk in ("node", "agent", "browser"):
            with self.subTest(sdk=sdk):
                manifest = json.loads(
                    (ROOT / "sdks" / sdk / "package.json").read_text()
                )
                self.assertEqual(manifest["engines"]["node"], ">=22.12.0")
        staging = (ROOT / "crates/xmtp_sdk/dev/stage-package.mjs").read_text()
        self.assertIn('engines: { node: ">=22.12.0" }', staging)


if __name__ == "__main__":
    unittest.main()
