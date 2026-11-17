"""Check credential provisioning without contacting a real sandbox."""

import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch


SCRIPT = Path(__file__).resolve().parents[2] / ".agents/skills/security-hardening/scripts/provision.py"
SPEC = importlib.util.spec_from_file_location("security_provision", SCRIPT)
assert SPEC and SPEC.loader
provision = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(provision)


class ProvisionTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.path = Path(temporary.name) / ".env"
        self.path.write_text(
            "OPENAI_API_KEY='native-secret'\nMORPH_API_KEY=custom-secret\n"
        )
        self.path.chmod(0o600)

    def test_selected_keys_only_reach_proxy_stdin(self):
        keys = provision.load_keys(self.path)
        selected = provision.parse_selection(
            ["openai:OPENAI_API_KEY"],
            ["api.morphllm.com:sbx-morph-libxmtp:MORPH_API_KEY"],
            keys,
        )
        calls = []

        def fake_run(command, **kwargs):
            calls.append((command, kwargs))
            return subprocess.CompletedProcess(command, 0)

        with patch.object(provision.subprocess, "run", side_effect=fake_run):
            provision.register(selected)

        self.assertEqual(len(calls), 2)
        self.assertEqual(calls[0][0], ["sbx", "secret", "set", "openai", "--sandbox", "libxmtp-claude"])
        self.assertEqual(calls[1][0][1:], ["key", "api.morphllm.com", "sbx-morph-libxmtp"])
        self.assertEqual([call[1]["input"] for call in calls], ["native-secret\n", "custom-secret\n"])
        for command, options in calls:
            self.assertNotIn("native-secret", " ".join(command))
            self.assertNotIn("custom-secret", " ".join(command))
            self.assertTrue(options["capture_output"])

    def test_invalid_file_or_selection_stops_before_registration(self):
        self.path.chmod(0o644)
        with self.assertRaisesRegex(ValueError, "group or others"):
            provision.load_keys(self.path)
        self.path.chmod(0o600)
        keys = provision.load_keys(self.path)
        with self.assertRaisesRegex(ValueError, "missing selected key"):
            provision.parse_selection(
                ["openai:OPENAI_API_KEY"],
                ["api.example.com:sbx-example:MISSING_KEY"],
                keys,
            )
        self.path.write_text("OPENAI_API_KEY=$(touch /tmp/should-not-exist)\n")
        with self.assertRaisesRegex(ValueError, "whitespace-containing"):
            provision.load_keys(self.path)


if __name__ == "__main__":
    unittest.main()
