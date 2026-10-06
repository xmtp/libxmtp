#!/usr/bin/env python3
"""Run the Windows workflow's cache cleanup and receipt steps."""

import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


def workflow_step(name):
    lines = (ROOT / ".github/workflows/test-sdk.yml").read_text().splitlines()
    start = lines.index("      - name: " + name)
    body = []
    collecting = False
    for line in lines[start + 1 :]:
        if line.startswith("      - "):
            break
        if line == "        run: |":
            collecting = True
            continue
        if collecting:
            body.append(line[10:])
    return "\n".join(body)


class WindowsStaging(unittest.TestCase):
    def test_cached_browser_roots_do_not_block_native_receipt(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder).resolve()
            debug = root / "target/debug"
            debug.mkdir(parents=True)
            (debug / "xmtp_sdk.dll").write_bytes(b"native fixture")
            generator = debug / "xmtp-sdk-bindgen.exe"
            generator.write_text("""#!/usr/bin/env bash
while [ "$#" -gt 0 ]; do
  if [ "$1" = --out ]; then
    shift
    output=$1
  fi
  shift
done
mkdir -p "$output"
printf 'generated fixture\\n' > "$output/index.ts"
""")
            generator.chmod(0o755)
            generated = root / "target/sdk-generated"
            for language in (
                "typescript-napi",
                "typescript-wasm",
                "typescript-pure",
                "swift",
                "kotlin",
            ):
                tree = generated / language
                tree.mkdir(parents=True)
                (tree / "cached.txt").write_text("old generated output")
            dev = root / "crates/xmtp_sdk/dev"
            dev.parent.mkdir(parents=True)
            dev.symlink_to(ROOT / "crates/xmtp_sdk/dev", target_is_directory=True)
            environment = dict(os.environ, PYTHONDONTWRITEBYTECODE="1")
            subprocess.run(
                ["bash", "-c", workflow_step("Generate TypeScript bindings")],
                cwd=root,
                env=environment,
                check=True,
            )
            step = workflow_step("Stage Windows ESM package")
            command = next(
                line for line in step.splitlines() if line.startswith("python ")
            )
            result = subprocess.run(
                ["bash", "-c", command],
                cwd=root,
                env=environment,
                text=True,
                capture_output=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            record = json.loads(
                (generated / "typescript-napi/sdk-contract.json").read_text()
            )
            self.assertEqual(record["artifact"]["profile"], "debug")
            self.assertIn("index.ts", record["files"])
            self.assertIn("xmtp_sdk.dll", record["files"])
            self.assertNotIn("cached.txt", record["files"])
            self.assertEqual(
                {path.name for path in generated.iterdir()}, {"typescript-napi"}
            )


if __name__ == "__main__":
    unittest.main()
