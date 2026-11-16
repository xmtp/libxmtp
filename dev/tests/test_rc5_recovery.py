import copy
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location(
    "recovery", ROOT / "dev/verify_rc5_recovery.py"
)
recovery = importlib.util.module_from_spec(spec)
spec.loader.exec_module(recovery)


class Rc5RecoveryTest(unittest.TestCase):
    def fixture(self):
        return (
            {
                "id": recovery.RUN,
                "head_sha": recovery.SOURCE,
                "head_branch": "release/8.0.0",
                "event": "workflow_dispatch",
                "path": ".github/workflows/release.yml",
                "repository": {"full_name": "xmtp/libxmtp"},
            },
            [
                {
                    "jobs": [
                        {
                            "name": "All six SDKs and selected CLI validated",
                            "conclusion": "success",
                        }
                    ]
                }
            ],
            {
                "sourceSha": recovery.SOURCE,
                "version": "8.0.0-rc5",
                "targetVersion": "8.0.0",
                "kind": "rc",
                "rcNumber": 5,
            },
        )

    def test_rejects_changed_source_version_origin_or_validation(self):
        for section, key, value in (
            (0, "id", 1),
            (0, "head_sha", "a" * 40),
            (0, "head_branch", "untrusted"),
            (0, "event", "pull_request"),
            (0, "path", ".github/workflows/other.yml"),
            (0, "repository", {"full_name": "fork/libxmtp"}),
            (2, "sourceSha", "b" * 40),
            (2, "version", "8.0.0-rc6"),
            (2, "targetVersion", "8.1.0"),
            (2, "kind", "dev"),
            (2, "rcNumber", 6),
        ):
            with self.subTest(section=section, key=key):
                args = copy.deepcopy(self.fixture())
                args[section][key] = value
                with self.assertRaises(ValueError):
                    recovery.verify(*args)
        for jobs in (
            [],
            [{"jobs": []}],
            [
                {
                    "jobs": [
                        {
                            "name": "All six SDKs and selected CLI validated",
                            "conclusion": "skipped",
                        }
                    ]
                }
            ],
        ):
            with self.subTest(jobs=jobs), self.assertRaises(ValueError):
                run, _, plan = self.fixture()
                recovery.verify(run, jobs, plan)

    def test_accepts_original_completed_validation(self):
        recovery.verify(*self.fixture())
