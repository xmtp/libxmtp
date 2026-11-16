import importlib.util
import io
import json
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location(
    "npm_publication", ROOT / "dev/publish_npm.py"
)
publisher = importlib.util.module_from_spec(spec)
spec.loader.exec_module(publisher)


class NpmPublicationTest(unittest.TestCase):
    def test_publish_acknowledgement_without_public_package_is_not_success(self):
        with tempfile.TemporaryDirectory() as folder:
            tarball = Path(folder) / "package.tgz"
            data = json.dumps(
                {"name": "@xmtp/node-sdk", "version": "8.0.0-rc5"}
            ).encode()
            with tarfile.open(tarball, "w:gz") as archive:
                entry = tarfile.TarInfo("package/package.json")
                entry.size = len(data)
                archive.addfile(entry, io.BytesIO(data))

            def npm(args, **kwargs):
                if args[1] == "publish":
                    return subprocess.CompletedProcess(args, 0)
                return subprocess.CompletedProcess(
                    args, 1, json.dumps({"error": {"code": "E404"}}), ""
                )

            with (
                patch.object(publisher.subprocess, "run", side_effect=npm),
                patch("time.sleep"),
            ):
                with self.assertRaisesRegex(ValueError, "visible"):
                    publisher.publish(tarball, "8.0.0-rc5", "prerelease", "node-sdk")
