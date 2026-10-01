"""Hash and size the complete installed distribution, including dependencies."""

import gzip
import hashlib
import io
import os
import tarfile
from pathlib import Path

from fixtures import digest

ROLES = {
    "swift": {"public", "native"},
    "kotlin": {"public", "native"},
    "node": {"public", "native", "runtime"},
    "browser": {"public", "worker", "wasm", "pure", "runtime"},
}


def inventory(root, assets, target):
    root = Path(root).resolve(strict=True)
    if not root.is_dir():
        raise ValueError("Package root must be a complete installed directory")
    rows = []
    archive = io.BytesIO()
    # A deterministic tar.gz comparison uses the same codec for both packages.
    with gzip.GzipFile(
        fileobj=archive, mode="wb", mtime=0, filename="", compresslevel=9
    ) as zipped:
        with tarfile.open(fileobj=zipped, mode="w|", format=tarfile.PAX_FORMAT) as tar:
            for base, directories, files in os.walk(root, followlinks=False):
                directories.sort()
                for name in sorted(directories + files):
                    path = Path(base) / name
                    if path.is_symlink():
                        raise ValueError(
                            f"Materialize package symlink before measurement: {path}"
                        )
                for name in sorted(files):
                    path = Path(base) / name
                    if not path.is_file():
                        raise ValueError(f"Package entry is not a regular file: {path}")
                    relative = path.relative_to(root).as_posix()
                    content = path.read_bytes()
                    mode = 0o755 if path.stat().st_mode & 0o111 else 0o644
                    rows.append(
                        {
                            "path": relative,
                            "bytes": len(content),
                            "mode": mode,
                            "sha256": hashlib.sha256(content).hexdigest(),
                        }
                    )
                    info = tarfile.TarInfo(relative)
                    info.size, info.mode, info.mtime = len(content), mode, 0
                    tar.addfile(info, io.BytesIO(content))
    rows.sort(key=lambda row: row["path"])
    paths = {row["path"] for row in rows}
    if not rows or sum(row["bytes"] for row in rows) == 0:
        raise ValueError("Package is empty")
    if set(assets) != ROLES[target]:
        raise ValueError(
            f"Declare the complete asset roles for {target}: {sorted(ROLES[target])}"
        )
    for role, names in assets.items():
        if not names or any(name not in paths for name in names):
            raise ValueError(f"Missing installed {role} asset")
    return {
        "files": rows,
        "sha256": digest(rows),
        "raw_bytes": sum(row["bytes"] for row in rows),
        "compressed_bytes": len(archive.getvalue()),
        "compression": "tar+gzip-9-mtime0",
    }
