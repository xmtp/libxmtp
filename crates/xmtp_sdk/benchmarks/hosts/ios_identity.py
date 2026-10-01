"""Hash app and build inputs without following a directory symlink."""

import hashlib
import os
from pathlib import Path

from fixtures import digest


def tree_identity(root):
    root = Path(root).resolve(strict=True)
    rows = []
    for base, directories, files in os.walk(root, followlinks=False):
        directories.sort()
        for name in sorted(directories + files):
            path = Path(base) / name
            relative = path.relative_to(root).as_posix()
            if path.is_symlink():
                rows.append({"path": relative, "link": os.readlink(path)})
            elif path.is_file():
                rows.append(
                    {
                        "path": relative,
                        "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                        "executable": bool(path.stat().st_mode & 0o111),
                    }
                )
    if not rows:
        raise ValueError(f"Empty app or build input: {root}")
    return digest(sorted(rows, key=lambda row: row["path"]))


def dependency_identity(root):
    """Bind dependency source and binary bytes, excluding mutable Git metadata."""
    root = Path(root)
    rows = []
    for base, directories, files in os.walk(root, followlinks=True):
        directories[:] = sorted(name for name in directories if name != ".git")
        for name in sorted(files):
            if name == ".git":
                continue
            path = Path(base) / name
            rows.append(
                {
                    "path": path.relative_to(root).as_posix(),
                    "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                }
            )
    return digest(sorted(rows, key=lambda row: row["path"]))
