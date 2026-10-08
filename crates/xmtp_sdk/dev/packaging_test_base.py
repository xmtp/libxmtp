"""Shared fixture for the SDK packaging test suites."""

import argparse
import importlib.util
from pathlib import Path
import tempfile
from unittest.mock import patch


def load(name, file):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(file))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


artifacts = load("artifacts", "sdk-artifacts.py")
receipt = load("receipt", "record-generated.py")
mobile = load("mobile", "mobile-package.py")
android_inputs = load("android_inputs", "sdk-packaging-android-inputs.py")


class PackagingTestBase:
    """Patch the packaging scripts onto a temporary checkout with fake tools."""

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name).resolve()
        (self.root / "crates/xmtp_sdk").mkdir(parents=True)
        (self.root / "apps/xmtp_sdk_bindgen").mkdir(parents=True)
        (self.root / "Cargo.toml").write_text("[workspace]\nmembers=[]\n")
        (self.root / "apps/xmtp_sdk_bindgen/template.txt").write_text("template")
        self.config = self.root / "crates/xmtp_sdk/uniffi.toml"
        self.config.write_text("fixture configuration")
        self.calls = []
        self.sdk_root = self.root / "sdks/android"
        self.patches = [
            patch.object(artifacts, "ROOT", self.root),
            patch.object(receipt.artifacts, "ROOT", self.root),
            patch.object(artifacts, "build_context", return_value="fixture compiler"),
            patch.object(artifacts, "run", side_effect=self.command),
            patch.object(mobile.artifacts, "ROOT", self.root),
        ]
        for item in self.patches:
            item.start()
        self.args = argparse.Namespace(
            artifacts=self.root / "compiled",
            targets=("swift",),
            out=self.root / "generated",
            features="",
            rust_target="",
            skip_bindgen=False,
            reuse_bindgen=None,
            profile="release",
            no_format=True,
        )

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
            folder = Path(command[command.index("--out") + 1])
            folder.mkdir(parents=True, exist_ok=True)
            language = command[command.index("--language") + 1]
            name = "xmtp_sdk.kt" if language == "kotlin" else "xmtp_sdk.swift"
            (folder / name).write_text("fixture binding")
