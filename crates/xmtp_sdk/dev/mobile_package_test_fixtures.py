"""Create and compare files for mobile package fixtures."""


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
