#!/usr/bin/env python3
"""Run the Windows workflow's cache cleanup and render step."""

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
    def test_cached_roots_are_removed_before_the_node_render(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder).resolve()
            debug = root / "target/debug"
            debug.mkdir(parents=True)
            (debug / "xmtp_sdk.dll").write_bytes(b"native fixture")
            arguments = root / "render-arguments.txt"
            generator = debug / "xmtp-sdk-bindgen.exe"
            generator.write_text(f"""#!/usr/bin/env bash
printf '%s\\n' "$@" > {arguments}
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
            # The job reads its identities from `sdk-artifacts.py provenance`,
            # whose Windows Python output ends lines with CRLF.
            tools = root / "tools"
            tools.mkdir()
            (tools / "python").write_text(
                "#!/usr/bin/env bash\nprintf 'commit:abc commit:abc\\r\\n'\n"
            )
            (tools / "python").chmod(0o755)
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
            step = workflow_step("Render and stage Windows ESM package")
            render = "\n".join(
                line for line in step.splitlines() if not line.startswith("node ")
            )
            subprocess.run(
                ["bash", "-c", render],
                cwd=root,
                env=dict(os.environ, PATH=f"{tools}{os.pathsep}{os.environ['PATH']}"),
                check=True,
            )
            self.assertEqual(
                {path.name for path in generated.iterdir()}, {"typescript-napi"}
            )
            self.assertEqual(
                [path.name for path in (generated / "typescript-napi").iterdir()],
                ["index.ts"],
            )
            passed = arguments.read_bytes().decode().split("\n")
            for flag, value in (
                ("--target", "node"),
                ("--profile", "debug"),
                ("--source", "commit:abc"),
                ("--generator", "commit:abc"),
                ("--oxfmt", "node_modules/.bin/oxfmt.cmd"),
            ):
                self.assertEqual(passed[passed.index(flag) + 1], value)


if __name__ == "__main__":
    unittest.main()
