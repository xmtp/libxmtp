"""Keep tracked files within 1 MiB, apart from reviewed existing assets."""

from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
LIMIT = 1024 * 1024
# Existing diagram; keep the exception exact so other images are still checked.
ALLOWLIST = {"img/mls-state-machine.png"}


def check(root, allowlist=ALLOWLIST):
    paths = subprocess.check_output(
        ["git", "ls-files", "-z"], cwd=root, text=True
    ).split("\0")
    failures = []
    for path in filter(None, paths):
        file = root / path
        if path in allowlist or file.is_symlink() or not file.is_file():
            continue
        size = file.stat().st_size
        if size > LIMIT:
            failures.append(f"{path}: {size} bytes exceeds {LIMIT} byte limit")
    return failures


def main():
    failures = check(ROOT)
    if failures:
        print("\n".join(failures), file=sys.stderr)
        return 1
    print("Tracked file sizes are within the limit.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
