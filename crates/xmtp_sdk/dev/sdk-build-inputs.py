#!/usr/bin/env python3.11
"""Read Cargo inputs and check the debug semantics used by CI SDK products."""

import hashlib
import os
from pathlib import Path
import re
import shutil
import tomllib

FALSE = {"n", "no", "off", "false"}
INCLUDE = re.compile(r'include_(?:str|bytes)!\s*\(\s*"([^"\n]+)"\s*\)')
ENV = re.compile(r'(?:option_)?env!\s*\(\s*"([A-Za-z_][A-Za-z_0-9]*)"')
RERUN_ENV = re.compile(r"cargo:rerun-if-env-changed=([A-Za-z_][A-Za-z_0-9]*)")
READ_FILE = re.compile(r'(?:fs::)?(?:read|read_to_string)!?\s*\(\s*"([^"\n]+)"')
RERUN_FILE = re.compile(r'cargo:rerun-if-changed=([^"{}\n]+)')


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def merge(first, second):
    result = dict(first)
    for name, value in second.items():
        old = result.get(name)
        if isinstance(old, dict) and isinstance(value, dict):
            result[name] = merge(old, value)
        elif isinstance(old, list) and isinstance(value, list):
            result[name] = old + value
        else:
            result[name] = value
    return result


def cargo_config(root):
    root = root.resolve()
    home = Path(os.environ.get("CARGO_HOME") or Path.home() / ".cargo").resolve()
    directories = [home, *(path / ".cargo" for path in reversed((root, *root.parents)))]
    configs = []
    seen = set()
    for directory in directories:
        # Cargo uses the extensionless file when both names are present.
        selected = next(
            (
                directory / name
                for name in ("config", "config.toml")
                if (directory / name).is_file()
            ),
            None,
        )
        if selected is not None and selected not in seen:
            seen.add(selected)
            configs.append(selected)
    hashes = {}
    effective = {}
    visiting = set()

    def read(path):
        if path in visiting:
            raise ValueError("Cargo configuration include cycle")
        visiting.add(path)
        try:
            try:
                data = tomllib.loads(path.read_text())
            except (OSError, tomllib.TOMLDecodeError):
                raise ValueError("Cargo configuration cannot be parsed") from None
            label = (
                "workspace:" + path.relative_to(root).as_posix()
                if path.is_relative_to(root)
                else str(path)
            )
            hashes[label] = digest(path)
            included = data.pop("include", [])
            if isinstance(included, str):
                included = [included]
            combined = {}
            for item in included:
                if not isinstance(item, str):
                    raise ValueError("Unsupported Cargo configuration include")
                combined = merge(combined, read((path.parent / item).resolve()))
            return merge(combined, data)
        finally:
            visiting.remove(path)

    for path in configs:
        effective = merge(effective, read(path))
    return hashes, effective


def compiler(root):
    _, config = cargo_config(root)
    selected = (
        os.environ.get("RUSTC")
        or os.environ.get("CARGO_BUILD_RUSTC")
        or config.get("build", {}).get("rustc")
        or "rustc"
    )
    if not isinstance(selected, str):
        raise ValueError("Invalid Cargo compiler selection")
    executable = shutil.which(selected)
    return (
        executable or str(root / selected)
        if "/" in selected and not Path(selected).is_absolute()
        else selected
    )


def rust_sources(root):
    for directory in ("crates", "apps", "bindings"):
        base = root / directory
        if base.exists():
            yield from (
                path
                for path in base.rglob("*.rs")
                if "target" not in path.parts and "node_modules" not in path.parts
            )


def declared_inputs(root):
    files = {}
    names = {"CI", "XMTP_TEST_LOGGING", "VERGEN_GIT_SHA"}
    workspace_path = False
    for source in rust_sources(root):
        text = source.read_text()
        file_names = INCLUDE.findall(text)
        if source.name == "build.rs":
            file_names += READ_FILE.findall(text) + RERUN_FILE.findall(text)
        for name in file_names:
            path = (source.parent / name).resolve()
            if path.is_file():
                label = (
                    path.relative_to(root).as_posix()
                    if path.is_relative_to(root)
                    else str(path)
                )
                files[label] = digest(path)
        for name in (*ENV.findall(text), *RERUN_ENV.findall(text)):
            if name == "CARGO_MANIFEST_DIR":
                workspace_path = True
            elif name not in ("OUT_DIR",) and not name.startswith("CARGO_PKG_"):
                names.add(name)
    environment = {
        name: hashlib.sha256(
            ("unset" if name not in os.environ else "set:" + os.environ[name]).encode()
        ).hexdigest()
        for name in sorted(names)
    }
    return files, environment, str(root.resolve()) if workspace_path else None


def cache_contract_supported(root):
    # Current generated proto bytes come from the hashed proto tree. Other
    # dynamic include paths need a new input contract before raw cache reuse.
    for source in rust_sources(root):
        if source.name.endswith("_test.rs") or "tests" in source.parts:
            continue
        text = source.read_text()
        starts = re.finditer(r"include_(?:str|bytes)!\s*\(\s*", text)
        for match in starts:
            if text[match.end() :].startswith('"'):
                continue
            if source.relative_to(
                root
            ).as_posix() == "crates/xmtp_proto/src/lib.rs" and text[
                match.end() :
            ].startswith('concat!(env!("OUT_DIR"), "/proto_descriptor.bin")'):
                continue
            return False
    return True


def tokens(value, encoded=False):
    if isinstance(value, list):
        return value
    if not isinstance(value, str):
        raise ValueError("Invalid Cargo Rust flags")
    return value.split("\x1f") if encoded else value.split()


def debug_flags_valid(values):
    for value in values:
        flags = tokens(value)
        options = []
        index = 0
        while index < len(flags):
            flag = flags[index]
            if flag == "-C":
                index += 1
                if index < len(flags):
                    options.append(flags[index])
            elif flag.startswith("-C"):
                options.append(flag[2:])
            elif "instrument-coverage" in flag:
                return False
            index += 1
        for option in options:
            name, _, setting = option.partition("=")
            if name == "debug-assertions" and setting.lower() in FALSE:
                return False
            if name == "panic" and setting == "abort":
                return False
            if name in ("opt-level", "instrument-coverage"):
                return False
    return True


def compiler_flags(root):
    _, config = cargo_config(root)
    values = {
        "RUSTFLAGS": tokens(os.environ.get("RUSTFLAGS", "")),
        "CARGO_ENCODED_RUSTFLAGS": tokens(
            os.environ.get("CARGO_ENCODED_RUSTFLAGS", ""), True
        ),
    }
    values["cargo-build"] = tokens(config.get("build", {}).get("rustflags", []))
    for target, settings in config.get("target", {}).items():
        if "rustflags" in settings:
            values["cargo-target:" + target] = tokens(settings["rustflags"])
    return values


def debug_profile_valid(root):
    _, config = cargo_config(root)
    profiles = config.get("profile", {}).get("dev", {})
    selected = [
        profiles,
        *(value for name, value in profiles.get("package", {}).items() if name != "*"),
    ]
    for profile in selected:
        if (
            profile.get("debug-assertions") is False
            or str(profile.get("debug-assertions", "true")).lower() in FALSE
        ):
            return False
        if profile.get("panic") == "abort" or profile.get("opt-level", 0) not in (
            0,
            "0",
        ):
            return False
    return debug_flags_valid(compiler_flags(root).values())


def require_debug_profile(root):
    if any(
        name.startswith("CARGO_PROFILE_") for name in os.environ
    ) or not debug_profile_valid(root):
        raise ValueError("CI SDK products require the default debug compiler profile")
