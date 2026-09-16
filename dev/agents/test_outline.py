"""Check the pinned upstream outline behavior that agents depend on."""

from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class OutlineTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="outline fixtures ")
        self.addCleanup(self.temp.cleanup)
        self.directory = Path(self.temp.name)
        self.cli = shutil.which("ast-outline")
        self.assertIsNotNone(
            self.cli, "Run just agent-test in the locked agent environment"
        )

    def fixture(self, name, source):
        path = self.directory / name
        path.write_text(source)
        return path

    def outline(self, path):
        return subprocess.check_output(
            [self.cli, "--no-docs", "--no-fields", "--no-attrs", str(path)],
            text=True,
        )

    def test_rust_alias_and_multiline_signature(self):
        path = self.fixture(
            "sample.rs",
            "pub type InboxId = String;\npub fn length(\n    value: &str,\n) -> usize {\n    value.len()\n}\n",
        )
        result = self.outline(path)
        self.assertIn("pub type InboxId = String", result)
        self.assertIn("value: &str", result)
        self.assertIn("-> usize", result)
        self.assertIn("L2-6", result)
        self.assertNotIn("value.len()", result)

    def test_javascript_and_typescript(self):
        for extension in ("js", "ts"):
            with self.subTest(extension=extension):
                path = self.fixture(
                    f"sample.{extension}",
                    "export function greet(name) { return name; }\nexport const twice = (n) => n * 2;\n",
                )
                result = self.outline(path)
                self.assertIn("greet", result)
                self.assertIn("twice", result)

    def test_kotlin_companion_and_symbol_read(self):
        path = self.fixture(
            "sample.kt",
            "class Client {\n    companion object {\n        fun create(): Client {\n            return Client()\n        }\n    }\n}\n",
        )
        self.assertIn("fun create(): Client", self.outline(path))
        result = subprocess.check_output(
            [self.cli, "show", str(path), "create"], text=True
        )
        self.assertIn("return Client()", result)

    def test_swift_signature(self):
        path = self.fixture(
            "sample.swift",
            "struct Client {\n    func send(\n        text: String\n    ) -> Bool {\n        return true\n    }\n}\n",
        )
        result = self.outline(path)
        self.assertIn("text: String", result)
        self.assertIn("-> Bool", result)
        self.assertNotIn("return true", result)

    def test_missing_file_and_symbol_have_visible_notes(self):
        missing = self.outline(self.directory / "missing.rs")
        self.assertIn("# note:", missing)
        path = self.fixture("empty.rs", "")
        result = subprocess.check_output(
            [self.cli, "show", str(path), "absent"], text=True
        )
        self.assertIn("symbol not found", result)

    def test_just_preserves_paths_and_multiple_arguments(self):
        paths = [
            self.fixture("one ' $file.rs", "pub fn first() {}\n"),
            self.fixture("two file.rs", "pub fn second() {}\n"),
        ]
        result = subprocess.check_output(
            ["just", "outline", *map(str, paths)],
            cwd=ROOT,
            text=True,
        )
        self.assertIn("first", result)
        self.assertIn("second", result)
        result = subprocess.check_output(
            ["just", "show", str(paths[0]), "first"],
            cwd=ROOT,
            text=True,
        )
        self.assertIn("pub fn first() {}", result)
