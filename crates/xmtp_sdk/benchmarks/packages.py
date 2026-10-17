"""Measure the raw and compressed size of a staged SDK package."""

import gzip
import hashlib
import io
import tarfile
from pathlib import Path


def measure_package(path):
    """Size every regular file under `path` (or the one file `path`).

    Compression is a deterministic tar archive with gzip level 9, so the same
    package bytes always give the same compressed size.
    """
    path = Path(path).resolve(strict=True)
    base = path.parent if path.is_file() else path
    files = [path] if path.is_file() else sorted(p for p in path.rglob("*"))
    archive = io.BytesIO()
    raw = count = 0
    content_hash = hashlib.sha256()
    with gzip.GzipFile(fileobj=archive, mode="wb", mtime=0, compresslevel=9) as zipped:
        with tarfile.open(fileobj=zipped, mode="w|", format=tarfile.PAX_FORMAT) as tar:
            for file in files:
                if file.is_symlink():
                    raise ValueError(f"Package has a symbolic link: {file}")
                if not file.is_file():
                    continue
                relative = file.relative_to(base).as_posix()
                content = file.read_bytes()
                info = tarfile.TarInfo(relative)
                info.size, info.mtime = len(content), 0
                info.mode = 0o755 if file.stat().st_mode & 0o111 else 0o644
                tar.addfile(info, io.BytesIO(content))
                content_hash.update(f"{relative}\0{len(content)}\0".encode())
                content_hash.update(hashlib.sha256(content).digest())
                raw += len(content)
                count += 1
    if raw == 0:
        raise ValueError(f"Package is empty: {path}")
    return {
        "path": str(path),
        "files": count,
        "raw_bytes": raw,
        "compressed_bytes": len(archive.getvalue()),
        "sha256": content_hash.hexdigest(),
    }
