#!/usr/bin/env python3.11
"""Register selected host-only dotenv keys with the libxmtp sbx proxy."""

import argparse
import os
from pathlib import Path
import re
import stat
import subprocess
import sys


SANDBOX = "libxmtp-claude"
NATIVE = {"openai", "anthropic"}
NAME = re.compile(r"[A-Za-z_][A-Za-z0-9_]*\Z")
HOST = re.compile(r"[A-Za-z0-9][A-Za-z0-9.-]*\Z")
PLACEHOLDER = re.compile(r"sbx-[A-Za-z0-9_-]+\Z")


def load_keys(path: Path) -> dict[str, str]:
    if path.is_symlink():
        raise ValueError("credential file must not be a symlink")
    metadata = path.stat()
    if not stat.S_ISREG(metadata.st_mode) or metadata.st_uid != os.getuid():
        raise ValueError("credential file must be a regular file owned by you")
    if metadata.st_mode & (stat.S_IRWXG | stat.S_IRWXO):
        raise ValueError("credential file must not be accessible by group or others")
    keys: dict[str, str] = {}
    for number, raw in enumerate(path.read_text().splitlines(), 1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        if line.startswith("export "):
            line = line[7:]
        if "=" not in line:
            raise ValueError(f"invalid dotenv entry at line {number}")
        name, value = line.split("=", 1)
        if not NAME.fullmatch(name) or name in keys:
            raise ValueError(f"invalid or duplicate dotenv name at line {number}")
        value = value.strip()
        if value.startswith(("'", '"')):
            quote = value[0]
            if len(value) < 2 or value[-1] != quote:
                raise ValueError(f"invalid quoted value at line {number}")
            value = value[1:-1]
        if not value or any(char.isspace() for char in value):
            raise ValueError(f"empty or whitespace-containing key at line {number}")
        keys[name] = value
    return keys


def parse_selection(native: list[str], custom: list[str], keys: dict[str, str]):
    selected = []
    for item in native:
        parts = item.split(":")
        if len(parts) != 2 or parts[0] not in NATIVE or not NAME.fullmatch(parts[1]):
            raise ValueError("native entries must be openai:ENV_NAME or anthropic:ENV_NAME")
        service, variable = parts
        if variable not in keys:
            raise ValueError(f"missing selected key: {variable}")
        selected.append(("native", service, variable, keys[variable]))
    for item in custom:
        parts = item.split(":")
        if (
            len(parts) != 3
            or not HOST.fullmatch(parts[0])
            or not PLACEHOLDER.fullmatch(parts[1])
            or not NAME.fullmatch(parts[2])
        ):
            raise ValueError("custom entries must be HOST:sbx-PLACEHOLDER:ENV_NAME")
        host, placeholder, variable = parts
        if variable not in keys:
            raise ValueError(f"missing selected key: {variable}")
        selected.append(("custom", host, placeholder, variable, keys[variable]))
    if not selected:
        raise ValueError("select at least one provider")
    if len({item[1:3] for item in selected}) != len(selected):
        raise ValueError("a provider was selected twice")
    return selected


def register(selected) -> None:
    wrapper = Path(__file__).resolve().parents[4] / "dev/sbx/sandbox"
    for item in selected:
        if item[0] == "native":
            _, service, variable, value = item
            command = ["sbx", "secret", "set", service, "--sandbox", SANDBOX]
            label = f"{service} ({variable})"
        else:
            _, host, placeholder, variable, value = item
            command = [str(wrapper), "key", host, placeholder]
            label = f"{host} [{placeholder}] ({variable})"
        result = subprocess.run(
            command, input=value + "\n", text=True, capture_output=True, check=False
        )
        if result.returncode != 0:
            raise RuntimeError(f"proxy registration failed for {label}; inspect sbx on the host")
        print(f"Registered {label}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--env-file", type=Path, required=True)
    parser.add_argument("--native", action="append", default=[], metavar="SERVICE:ENV_NAME")
    parser.add_argument(
        "--custom", action="append", default=[], metavar="HOST:PLACEHOLDER:ENV_NAME"
    )
    args = parser.parse_args()
    try:
        register(parse_selection(args.native, args.custom, load_keys(args.env_file)))
    except (OSError, ValueError, RuntimeError) as error:
        print(f"provision: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
