"""The SDK product check binds trees to the native library by identity."""

import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location(
    "check_sdk_products", Path(__file__).resolve().parents[1] / "check-sdk-products.py"
)
check = importlib.util.module_from_spec(spec)
spec.loader.exec_module(check)


class ReceiptTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.tree = Path(directory.name)
        (self.tree / "xmtp_sdk.kt").write_text("generated\n")
        self.record = {
            "contract": "c" * 64,
            "generator": "render",
            "artifact": {
                "source": "source",
                "generator": "render",
                "features": "",
                "profile": "release",
                "target": "",
                # The bytes that the render read; another build differs.
                "files": {"libxmtp_sdk.so": hashlib.sha256(b"build A").hexdigest()},
            },
            "files": {"xmtp_sdk.kt": hashlib.sha256(b"generated\n").hexdigest()},
        }

    def receipt(self, **changes):
        (self.tree / "sdk-contract.json").write_text(json.dumps(self.record | changes))
        return check.receipt(self.tree, "source", "render", "libxmtp_sdk.so")

    def test_another_build_of_the_library_matches(self):
        self.assertEqual(self.receipt()["contract"], "c" * 64)

    def test_another_library_or_source_does_not_match(self):
        artifact = self.record["artifact"]
        for changes in (
            {"artifact": artifact | {"files": {"libxmtp_sdk.dylib": "d" * 64}}},
            {"artifact": artifact | {"source": "other"}},
            {"generator": "other"},
        ):
            with self.subTest(changes), self.assertRaises(ValueError):
                self.receipt(**changes)


class BindingTests(unittest.TestCase):
    def test_a_render_of_another_library_fails(self):
        with self.assertRaisesRegex(ValueError, "xmtp-sdk-generated-swift"):
            check.check_products(
                {
                    "bound": {
                        "xmtp-sdk-generated-kotlin": True,
                        "xmtp-sdk-generated-swift": False,
                    }
                }
            )


if __name__ == "__main__":
    unittest.main()
