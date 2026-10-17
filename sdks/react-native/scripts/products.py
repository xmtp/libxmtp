#!/usr/bin/env python3
"""Run the same declared Nix producer/exchange/assembly graph as CI."""

import argparse
import json
from pathlib import Path
import platform
import subprocess

ROOT = Path(__file__).resolve().parents[3]
EXCHANGE = ROOT / "nix/react-native-exchange.py"


def run(*args, **kwargs):
    return subprocess.run(list(map(str, args)), cwd=ROOT, check=True, **kwargs)


def exchange(command, stage, system, directory):
    result = run(
        "python3",
        EXCHANGE,
        command,
        "--stage",
        stage,
        "--system",
        system,
        "--directory",
        directory,
        stdout=subprocess.PIPE,
        text=True,
    )
    # The final line is the declared root map; import/export can print progress.
    return json.loads(result.stdout.strip().splitlines()[-1])


def link_package(roots, directory):
    product = Path(roots["xmtp-sdk-react-native-package"])
    receipt = json.loads((product / "sdk-contract.json").read_text())
    tarball = product / receipt["tarball"]
    link = directory / "package.tgz"
    if link.is_symlink():
        link.unlink()
    elif link.exists():
        raise ValueError(f"refusing to overwrite {link}")
    link.symlink_to(tarball)
    print(f"Canonical React Native tarball: {link}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="action", required=True)
    components = commands.add_parser("components")
    components.add_argument("platform", choices=("android", "ios"))
    components.add_argument("out", type=Path)
    package = commands.add_parser("package")
    package.add_argument("android", type=Path)
    package.add_argument("ios", type=Path)
    load = commands.add_parser("load-package")
    load.add_argument("directory", type=Path)
    args = parser.parse_args()
    if args.action == "components":
        expected_host = "Linux" if args.platform == "android" else "Darwin"
        if platform.system() != expected_host:
            raise ValueError(
                f"{args.platform} components must be produced on {expected_host}"
            )
        system = "x86_64-linux" if args.platform == "android" else "aarch64-darwin"
        run("nix", "build", "--no-link", f".#ciStages.{system}.components")
        exchange("export", "components", system, args.out)
    elif args.action == "package":
        if platform.system() != "Linux":
            raise ValueError("canonical package assembly runs on Linux")
        exchange("import", "components", "x86_64-linux", args.android)
        exchange("import", "components", "aarch64-darwin", args.ios)
        # Missing foreign-system paths fail after import; they cannot trigger an
        # opportunistic Darwin compilation on this assembly host.
        run("nix", "build", "--no-link", ".#xmtp-sdk-react-native-package")
        directory = ROOT / "target/rn-release/package"
        roots = exchange("export", "package", "x86_64-linux", directory)
        link_package(roots, directory)
    else:
        roots = exchange("import", "package", "x86_64-linux", args.directory)
        link_package(roots, args.directory)


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, KeyError, subprocess.CalledProcessError) as error:
        raise SystemExit(f"React Native product error: {error}") from error
