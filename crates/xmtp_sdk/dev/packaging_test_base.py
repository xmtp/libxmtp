"""Shared fixture for the SDK packaging test suites."""

import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
from unittest.mock import Mock, patch


def load(name, file):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(file))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


artifacts = load("artifacts", "sdk-artifacts.py")
mobile = load("mobile", "mobile-package.py")


# The real identities come from `nix eval .#sdkProvenance` (nix/lib/sdk-sources.nix).
# This stand-in keeps their shape on a fixture checkout: crate sources and
# embedded data are SDK source, and the bindgen and the configuration crate it
# compiles are generator source. Packaging files are in neither.
def fixture_identity(root, generator):
    def kept(name):
        if name.startswith("Cargo."):
            return True
        if generator:
            return name.startswith(
                ("apps/xmtp_sdk_bindgen/", "crates/xmtp_configuration/")
            )
        return name.startswith("crates/") and name.endswith(
            (".rs", ".toml", ".json", ".txt")
        )

    files = sorted(
        name
        for name in (str(path.relative_to(root)) for path in root.rglob("*"))
        if (root / name).is_file() and kept(name)
    )
    return hashlib.sha256(
        json.dumps([(name, artifacts.digest(root / name)) for name in files]).encode()
    ).hexdigest()


class PackagingTestBase:
    """Patch the packaging scripts onto a temporary checkout with fake tools."""

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name).resolve()
        (self.root / "crates/xmtp_sdk").mkdir(parents=True)
        (self.root / "apps/xmtp_sdk_bindgen").mkdir(parents=True)
        (self.root / "Cargo.toml").write_text("fixture manifest")
        (self.root / "apps/xmtp_sdk_bindgen/template.txt").write_text("template")
        self.config = self.root / "crates/xmtp_sdk/uniffi.toml"
        self.config.write_text("fixture configuration")
        self.calls = []
        self.sdk_root = self.root / "sdks/android"
        self.patches = [
            patch.object(artifacts, "ROOT", self.root),
            patch.object(artifacts, "provenance", side_effect=self.provenance),
            patch.object(artifacts, "build_context", return_value="fixture compiler"),
            patch.object(artifacts, "run", side_effect=self.command),
            patch.object(mobile.artifacts, "ROOT", self.root),
            patch.object(mobile.artifacts, "provenance", side_effect=self.provenance),
            patch.object(artifacts, "formatter", return_value=[]),
            patch.object(artifacts, "runtime_packages", return_value={}),
            patch.object(artifacts, "bindgen_route", return_value="cargo"),
        ]
        for item in self.patches:
            item.start()
        self.args = argparse.Namespace(
            artifacts=self.root / "compiled",
            targets=("swift",),
            out=self.root / "generated",
            rust_target="",
            skip_bindgen=False,
            profile="release",
        )

    def provenance(self):
        return {
            "source": fixture_identity(self.root, generator=False),
            "generator": fixture_identity(self.root, generator=True),
            "render": dict.fromkeys(
                ("swift", "kotlin", "typescript"),
                fixture_identity(self.root, generator=True),
            ),
        }

    def tearDown(self):
        for item in reversed(self.patches):
            item.stop()
        self.temporary.cleanup()

    def command(self, command, **kwargs):
        self.calls.append(command)
        if command[0] == "dev/agent-run":
            folder = Path(kwargs["env"]["CARGO_TARGET_DIR"])
            if "--target" in command:
                folder /= command[command.index("--target") + 1]
            folder /= "debug" if "xmtp-sdk-bindgen" in command else "release"
            folder.mkdir(parents=True, exist_ok=True)
            for name in (
                "libxmtp_sdk.a",
                "libxmtp_sdk.dylib",
                "libxmtp_sdk.so",
                "xmtp-sdk-bindgen",
            ):
                (folder / name).write_text("fixture binary")
        else:
            # A stand-in for `xmtp-sdk-bindgen render`: one binding file and
            # the receipt fields that mobile preflight reads.
            def value(flag):
                return command[command.index(flag) + 1]

            folder = Path(value("--out"))
            folder.mkdir(parents=True)
            name = "xmtp_sdk.kt" if value("--target") == "kotlin" else "xmtp_sdk.swift"
            (folder / name).write_text("fixture binding")
            library = Path(value("--lib"))
            artifact = {
                field: value("--" + flag)
                for field, flag in (
                    ("source", "source"),
                    ("generator", "generator"),
                    ("features", "features"),
                    ("profile", "profile"),
                    ("target", "rust-target"),
                )
            }
            artifact["files"] = {library.name: artifacts.digest(library)}
            (folder / "sdk-contract.json").write_text(
                json.dumps(
                    {
                        "generator": value("--generator"),
                        "artifact": artifact,
                        "files": {name: artifacts.digest(folder / name)},
                    }
                )
            )


# Android dependency-input cases for the packaging suite.
class AndroidDependencyInputs:
    def seed_android_dependency_inputs(self):
        project = self.sdk_root
        for name in (
            "library/gradle.lockfile",
            "buildscript-gradle.lockfile",
            "gradle/verification-metadata.xml",
        ):
            file = project / name
            file.parent.mkdir(parents=True, exist_ok=True)
            file.write_text("fixture dependency input")

    def test_android_owned_project_override_keeps_common_native_receipts(self):
        self.prepare_mobile_stage("android")
        self.sdk_root = self.root / "owned-worktree/sdks/android"
        self.seed_android_dependency_inputs()
        self.assemble_mobile("android", sdk_root=self.sdk_root)
        self.assertTrue((self.root / "products/android/xmtp-sdk.aar").exists())
        self.assertFalse((self.root / "sdks/android/library/build").exists())

    def test_android_dependency_inputs_are_required_before_tool_use(self):
        for name in (
            "library/gradle.lockfile",
            "buildscript-gradle.lockfile",
            "gradle/verification-metadata.xml",
        ):
            with self.subTest(name=name):
                output = self.prepare_mobile_stage("android")
                previous = self.product_files(output)
                project = self.sdk_root
                (project / name).unlink()
                tool = Mock(side_effect=self.mobile_tool)
                with self.assertRaisesRegex(
                    ValueError, "Android dependency input missing"
                ):
                    self.assemble_mobile("android", tool)
                tool.assert_not_called()
                self.assertEqual(self.product_files(output), previous)
                self.assertEqual(list(output.parent.glob(".sdk-mobile-stage-*")), [])

    def test_android_stage_uses_strict_read_only_dependency_inputs(self):
        self.prepare_mobile_stage("android")

        def tool(command, **kwargs):
            self.assertEqual(command[0], str(self.sdk_root / "gradlew"))
            self.assertEqual(command[2], str(self.sdk_root))
            self.assertIn(":library:assembleRelease", command)
            self.assertIn("--dependency-verification=strict", command)
            self.assertFalse(any(arg.startswith("--max-workers") for arg in command))
            self.assertIn("-Pkotlin.compiler.execution.strategy=in-process", command)
            self.assertFalse(
                any(
                    arg.startswith(
                        (
                            "--write-locks",
                            "--update-locks",
                            "--write-verification-metadata",
                        )
                    )
                    for arg in command
                )
            )
            self.mobile_tool(command, **kwargs)

        self.assemble_mobile("android", tool)
