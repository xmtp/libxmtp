#!/usr/bin/env python3
"""Record ELF load inputs and transport their referenced Nix runtime closure."""

from pathlib import Path
import re
import struct
import subprocess

STORE = re.compile(r"^(/nix/store/[a-z0-9]{32}-[^/]+)(?:/.*)?$")
ARCHIVE = "native-runtime.nar"


def elf_load_inputs(path):
    """Read dynamic load records. Ignore debug strings and build source paths."""
    data = path.read_bytes()
    if data[:4] != b"\x7fELF":
        return None
    if len(data) < 64 or data[4] != 2 or data[5] not in (1, 2):
        raise ValueError(f"unsupported ELF header: {path}")
    order = "<" if data[5] == 1 else ">"
    phoff = struct.unpack_from(order + "Q", data, 32)[0]
    phsize, count = struct.unpack_from(order + "HH", data, 54)
    if phsize < 56 or count > 4096 or phoff + phsize * count > len(data):
        raise ValueError(f"invalid ELF program headers: {path}")
    headers = [
        struct.unpack_from(order + "IIQQQQQQ", data, phoff + index * phsize)
        for index in range(count)
    ]
    entries = []
    interpreter = None
    for kind, _, offset, _, _, size, _, _ in headers:
        if offset + size > len(data):
            raise ValueError(f"invalid ELF segment: {path}")
        if kind == 3:
            interpreter = data[offset : offset + size].split(b"\0", 1)[0].decode()
        if kind == 2:
            for position in range(offset, offset + size, 16):
                if position + 16 > offset + size:
                    raise ValueError(f"invalid ELF dynamic section: {path}")
                tag, value = struct.unpack_from(order + "qQ", data, position)
                if tag == 0:
                    break
                entries.append((tag, value))
    address = next((value for tag, value in entries if tag == 5), None)
    size = next((value for tag, value in entries if tag == 10), None)
    string_offset = None
    if address is not None:
        for kind, _, offset, vaddr, _, file_size, _, _ in headers:
            if kind == 1 and vaddr <= address < vaddr + file_size:
                string_offset = offset + address - vaddr
                break
    strings = [value for tag, value in entries if tag in (1, 15, 29)]
    if strings and (
        string_offset is None or size is None or string_offset + size > len(data)
    ):
        raise ValueError(f"invalid ELF dynamic strings: {path}")

    def string(index):
        if index >= size:
            raise ValueError(f"invalid ELF string index: {path}")
        end = data.find(b"\0", string_offset + index, string_offset + size)
        if end < 0:
            raise ValueError(f"unterminated ELF string: {path}")
        return data[string_offset + index : end].decode()

    return {
        "needed": [string(value) for tag, value in entries if tag == 1],
        "searchPaths": [
            item
            for tag, value in entries
            if tag in (15, 29)
            for item in string(value).split(":")
        ],
        "interpreter": interpreter,
    }


def inspect(package, scan_paths=None):
    if scan_paths is not None and any(
        Path(name).is_absolute() or ".." in Path(name).parts or "\\" in name
        for name in scan_paths
    ):
        raise ValueError("unsafe native scan path")
    linkage = {}
    roots = set()
    selected = (
        sorted(package.rglob("*"))
        if scan_paths is None
        else [package / name for name in sorted(set(scan_paths))]
    )
    for path in selected:
        if scan_paths is not None:
            name = path.relative_to(package)
            if (
                name.is_absolute()
                or ".." in name.parts
                or "\\" in str(name)
                or not path.is_file()
                or not path.resolve().is_relative_to(package.resolve())
            ):
                raise ValueError("unsafe or missing native scan path")
        elif not path.is_file() or path.suffix not in (".so", ".node"):
            continue
        record = elf_load_inputs(path)
        if record is None:
            continue
        linkage[path.relative_to(package).as_posix()] = record
        for entry in (*record["needed"], *record["searchPaths"], record["interpreter"]):
            match = STORE.fullmatch(entry) if entry else None
            if match:
                roots.add(match[1])
    return {"linkage": linkage, "roots": sorted(roots)}


def hashes(paths):
    if not paths:
        return {}
    values = subprocess.check_output(
        ["nix-store", "--query", "--hash", *paths], text=True
    ).splitlines()
    if len(values) != len(paths):
        raise ValueError("native runtime store hash inventory mismatch")
    return dict(zip(paths, values))


def export(package, folder, scan_paths=None):
    record = inspect(package, scan_paths)
    store_paths = []
    if record["roots"]:
        store_paths = sorted(
            set(
                subprocess.check_output(
                    ["nix-store", "--query", "--requisites", *record["roots"]],
                    text=True,
                ).splitlines()
            )
        )
        if not set(record["roots"]) <= set(store_paths) or any(
            not STORE.fullmatch(path) or STORE.fullmatch(path)[1] != path
            for path in store_paths
        ):
            raise ValueError("native runtime closure inventory mismatch")
        with (folder / ARCHIVE).open("wb") as output:
            subprocess.run(
                ["nix-store", "--export", *store_paths], stdout=output, check=True
            )
    return {
        **record,
        **({"scanPaths": sorted(set(scan_paths))} if scan_paths is not None else {}),
        "storeHashes": hashes(store_paths),
        "archive": ARCHIVE if store_paths else None,
    }


def check(package, folder, record):
    expected = inspect(package, record.get("scanPaths"))
    if expected != {key: record[key] for key in ("linkage", "roots")}:
        raise ValueError("native runtime ELF load inputs mismatch")
    paths = set(record["storeHashes"])
    if (
        (not record["roots"] and paths)
        or not set(record["roots"]) <= paths
        or any(
            not STORE.fullmatch(path) or STORE.fullmatch(path)[1] != path
            for path in paths
        )
    ):
        raise ValueError("native runtime store paths mismatch")
    if record["archive"] != (ARCHIVE if paths else None):
        raise ValueError("native runtime closure archive mismatch")
    if paths and not (folder / ARCHIVE).is_file():
        raise ValueError("native runtime closure archive missing")


def restore(folder, record, gc_roots=None):
    if record["archive"]:
        # The caller validates the entire product before this runner-local write.
        # No cache, signature policy, or store trust setting is changed here.
        with (folder / ARCHIVE).open("rb") as source:
            subprocess.run(
                ["nix-store", "--import"],
                stdin=source,
                stdout=subprocess.DEVNULL,
                check=True,
            )
    if gc_roots is None:
        verify(record)
    else:
        retain(gc_roots, record)


def retain(gc_roots, record):
    # Verify existing outputs before --realise. No derivation is requested.
    verify(record)
    if record["roots"]:
        # Check files as well as store metadata before requesting GC roots.
        subprocess.run(
            ["nix-store", "--verify-path", *sorted(record["storeHashes"])],
            stdout=subprocess.DEVNULL,
            check=True,
        )
        gc_roots.mkdir(parents=True, exist_ok=True)
    for root in record["roots"]:
        link = gc_roots / Path(root).name
        subprocess.run(
            ["nix-store", "--realise", "--add-root", str(link), "--indirect", root],
            stdout=subprocess.DEVNULL,
            check=True,
        )


def verify(record):
    if record["roots"]:
        closure = set(
            subprocess.check_output(
                ["nix-store", "--query", "--requisites", *record["roots"]], text=True
            ).splitlines()
        )
        if closure != set(record["storeHashes"]):
            raise ValueError("native runtime closure inventory mismatch")
    if hashes(sorted(record["storeHashes"])) != record["storeHashes"]:
        raise ValueError("native runtime store hash mismatch")
