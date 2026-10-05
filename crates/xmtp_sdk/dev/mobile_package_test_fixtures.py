"""Create and compare files for mobile package fixtures."""

import json


class MobilePackageTestFixtures:
    def seed_android_dependency_inputs(self):
        project = self.root / "crates/xmtp_sdk/packaging/android"
        for name in (
            "gradle.lockfile",
            "buildscript-gradle.lockfile",
            "gradle/verification-metadata.xml",
        ):
            file = project / name
            file.parent.mkdir(parents=True, exist_ok=True)
            file.write_text("fixture dependency input")

    def product_files(self, output):
        return {
            str(path.relative_to(output)): path.read_bytes()
            for path in output.rglob("*")
            if path.is_file()
        }

    def test_ios_public_stage_includes_host_and_mobile_libraries(self):
        output = self.prepare_mobile_stage("ios")
        self.assemble_mobile("ios")
        contract = json.loads((output / "sdk-contract.json").read_text())
        self.assertEqual(
            set(contract["native"]),
            {"aarch64-apple-ios", "aarch64-apple-ios-sim", "aarch64-apple-darwin"},
        )
        self.assertIn(".macOS(.v11)", (output / "Package.swift").read_text())
