"""The installed Agent gate rejects a bad pin, runtime, or package export."""

import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class InstalledAgentTest(unittest.TestCase):
    def test_installed_tarball_must_pin_and_load_the_matching_native_runtime(self):
        with tempfile.TemporaryDirectory() as folder:
            directory = Path(folder)
            # Stub only npm's installation transport. The production checker
            # reads the retained package manifest and imports installed modules
            # in a real Node process, including its version/export assertions.
            npm = directory / "npm"
            npm.write_text(f"""#!{sys.executable}
import os, tarfile
from pathlib import Path
assert os.sys.argv[1] == 'install'
for path in (os.sys.argv[-1], os.environ['NODE_TARBALL']):
    with tarfile.open(path) as archive:
        import json
        name=json.load(archive.extractfile('package/package.json'))['name']
        destination=Path('node_modules') / name
        destination.mkdir(parents=True, exist_ok=True)
        for entry in archive.getmembers():
            (destination / entry.name.removeprefix('package/')).write_bytes(archive.extractfile(entry).read())
""")
            npm.chmod(0o755)

            def pack(path, manifest, source):
                with tarfile.open(path, "w:gz") as archive:
                    for name, content in {
                        "package.json": json.dumps(manifest),
                        "index.js": source,
                    }.items():
                        payload = content.encode()
                        info = tarfile.TarInfo("package/" + name)
                        info.size = len(payload)
                        archive.addfile(info, io.BytesIO(payload))

            agent, node = directory / "agent.tgz", directory / "node.tgz"
            for pin, native, export in [
                ("8.0.0-rc5", "8.0.0-rc5", True),
                ("latest", "8.0.0-rc5", True),
                ("8.0.0-rc5", "8.0.0-rc4", True),
                ("8.0.0-rc5", "8.0.0-rc5", False),
            ]:
                with self.subTest(pin=pin, native=native, export=export):
                    pack(
                        agent,
                        {
                            "name": "@xmtp/agent-sdk",
                            "version": "8.0.0-rc5",
                            "type": "module",
                            "main": "index.js",
                            "dependencies": {"@xmtp/node-sdk": pin},
                        },
                        "export class Agent {}"
                        if export
                        else "export const missing = true",
                    )
                    pack(
                        node,
                        {
                            "name": "@xmtp/node-sdk",
                            "version": "8.0.0-rc5",
                            "type": "module",
                            "main": "index.js",
                        },
                        f"export function sdkVersion() {{ return {json.dumps(native)} }}",
                    )
                    result = subprocess.run(
                        [
                            "node",
                            str(ROOT / "dev/release-tools/check-agent-package.mjs"),
                            str(agent),
                            str(node),
                        ],
                        capture_output=True,
                        text=True,
                        env=dict(
                            os.environ,
                            PATH=str(directory) + os.pathsep + os.environ["PATH"],
                            NODE_TARBALL=str(node),
                        ),
                    )
                    valid = pin == native == "8.0.0-rc5" and export
                    self.assertEqual(
                        result.returncode, 0 if valid else 1, result.stderr
                    )
                    if not valid:
                        self.assertIn("AssertionError", result.stderr)


if __name__ == "__main__":
    unittest.main()
