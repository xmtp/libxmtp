#!/usr/bin/env python3
"""Exchange the declared RN Nix products through a signed same-run cache.

The checkout determines expected product paths. The ephemeral key authenticates
transport bytes; the same-run workflow and its source determine their authority.
PR imports stay in CI jobs. Trusted writers accept only their own main run.
Jobs with a Cachix token skip the exchange and substitute instead; `roots`
prints the declared products for them. `--kind test` selects the stages of
the partial CI test package, and `--kind test-android` those of the same
package without iOS (the Android smoke).
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
from urllib.parse import urlencode
import uuid

STORE_PATH = re.compile(r"/nix/store/[0-9a-df-np-sv-z]{32}-[A-Za-z0-9+._?=-]+\Z")
NAR_URL = re.compile(r"nar/[A-Za-z0-9+._=-]+\Z")


def run(*args, **kwargs):
    return subprocess.run(args, check=True, **kwargs)


# Both kinds expose link farms whose passthru.members are the stage roots.
STAGES = {
    "release": "ciStages.{system}",
    "test": "legacyPackages.{system}.reactNative.testStages",
    "test-android": "legacyPackages.{system}.reactNative.androidTestStages",
}


def roots(system, stage, version=None, kind="release"):
    if version is not None:
        if stage != "package" or system != "x86_64-linux" or kind != "release":
            raise ValueError(
                "a release version applies only to the Linux release package stage"
            )
        result = run(
            "nix",
            "eval",
            "--json",
            "--impure",
            "--expr",
            '{ xmtp-sdk-react-native-package = ((builtins.getFlake ("git+file://" + toString ./.)).legacyPackages.x86_64-linux.reactNative.mkPackage { version = builtins.getEnv "XMTP_RN_PACKAGE_VERSION"; }).outPath; }',
            env={**os.environ, "XMTP_RN_PACKAGE_VERSION": version},
            stdout=subprocess.PIPE,
        )
    else:
        result = run(
            "nix",
            "eval",
            "--json",
            f".#{STAGES[kind].format(system=system)}.{stage}",
            "--apply",
            "set: builtins.mapAttrs (_: drv: drv.outPath) set.passthru.members",
            stdout=subprocess.PIPE,
        )
    result = json.loads(result.stdout)
    if not result:
        raise ValueError("the selected stage has no products on this system")
    return result


def info(paths, store=None):
    command = ["nix", "path-info", "--json", "--recursive"]
    if store is not None:
        command.extend(["--store", store])
    values = json.loads(run(*command, *paths, stdout=subprocess.PIPE).stdout)
    if isinstance(values, list):
        values = {value["path"]: value for value in values}
    return {
        path: {"narHash": value["narHash"], "references": sorted(value["references"])}
        for path, value in sorted(values.items())
    }


def digest(path):
    result = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            result.update(block)
    return result.hexdigest()


def inventory(directory):
    result = {}
    for path in sorted(directory.rglob("*")):
        if path.is_symlink():
            raise ValueError("export cache contains a symbolic link")
        if path.is_file():
            result[path.relative_to(directory).as_posix()] = digest(path)
    return result


def validate_graph(expected, closure):
    if not set(expected.values()) <= closure.keys():
        raise ValueError("export omitted a declared product")
    for path, value in closure.items():
        if (
            not STORE_PATH.fullmatch(path)
            or not set(value["references"]) <= closure.keys()
        ):
            raise ValueError(
                "export closure contains an invalid path or missing reference"
            )
    pending = list(expected.values())
    reachable = set()
    while pending:
        path = pending.pop()
        if path not in reachable:
            reachable.add(path)
            pending.extend(closure[path]["references"])
    if reachable != closure.keys():
        raise ValueError("export closure contains undeclared unreachable paths")


def validate_cache(cache, closure, expected_files):
    if cache.is_symlink() or inventory(cache) != expected_files:
        raise ValueError("export cache file hashes do not match its manifest")
    expected_narinfos = {Path(path).name[:32] + ".narinfo" for path in closure}
    if {path.name for path in cache.glob("*.narinfo")} != expected_narinfos:
        raise ValueError("export cache contains missing or undeclared narinfo paths")
    for name in expected_narinfos:
        lines = (cache / name).read_text().splitlines()
        urls = [
            line.removeprefix("URL: ") for line in lines if line.startswith("URL: ")
        ]
        if len(urls) != 1 or not NAR_URL.fullmatch(urls[0]) or ".." in urls[0]:
            raise ValueError("export narinfo URL must name a local NAR file")
        if not (cache / urls[0]).is_file():
            raise ValueError("export narinfo refers to a missing NAR file")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["export", "import", "roots"])
    parser.add_argument(
        "--system", required=True, choices=["x86_64-linux", "aarch64-darwin"]
    )
    parser.add_argument("--stage", required=True, choices=["components", "package"])
    parser.add_argument("--directory", type=Path)
    parser.add_argument(
        "--version",
        help="release workflow package version; native components are unchanged",
    )
    parser.add_argument("--kind", choices=sorted(STAGES), default="release")
    args = parser.parse_args()
    expected = roots(args.system, args.stage, args.version, args.kind)
    if args.command == "roots":
        print(json.dumps(expected, sort_keys=True))
        return
    if args.directory is None:
        parser.error(f"{args.command} needs --directory")
    directory = args.directory.resolve()
    cache = directory / "cache"
    cache_uri = cache.as_uri()
    manifest_path = directory / "manifest.json"
    if args.command == "export":
        directory.mkdir(parents=True, exist_ok=False)
        closure = info(list(expected.values()))  # Fail if a producer did not finish.
        validate_graph(expected, closure)
        # Never include the private key in the exported directory or retain it.
        with tempfile.TemporaryDirectory(prefix="rn-cache-key-") as secret_directory:
            private_key = Path(secret_directory).resolve() / "secret"
            with private_key.open("wb") as stream:
                private_key.chmod(0o600)
                run(
                    "nix",
                    "key",
                    "generate-secret",
                    "--key-name",
                    "rn-exchange-" + uuid.uuid4().hex,
                    stdout=stream,
                )
            with private_key.open("rb") as stream:
                public_key = (
                    run(
                        "nix",
                        "key",
                        "convert-secret-to-public",
                        stdin=stream,
                        stdout=subprocess.PIPE,
                    )
                    .stdout.decode()
                    .strip()
                )
            run(
                "nix",
                "copy",
                "--to",
                # Nix defaults to single-threaded xz for file:// caches.
                cache_uri
                + "?"
                + urlencode(
                    {
                        "secret-key": str(private_key),
                        "compression": "zstd",
                        "parallel-compression": "true",
                    }
                ),
                *expected.values(),
            )
        files = inventory(cache)
        validate_cache(cache, closure, files)
        manifest_path.write_text(
            json.dumps(
                {
                    "schema": 2,
                    "system": args.system,
                    "stage": args.stage,
                    "roots": expected,
                    "closure": closure,
                    "publicKey": public_key,
                    "cacheFiles": files,
                },
                sort_keys=True,
                indent=2,
            )
            + "\n"
        )
    else:
        manifest = json.loads(manifest_path.read_text())
        if (
            manifest.get("schema"),
            manifest.get("system"),
            manifest.get("stage"),
            manifest.get("roots"),
        ) != (2, args.system, args.stage, expected):
            raise ValueError(
                "export roots do not match this checkout's declared RN products"
            )
        closure = manifest["closure"]
        validate_graph(expected, closure)
        validate_cache(cache, closure, manifest["cacheFiles"])
        key = manifest["publicKey"]
        if not re.fullmatch(r"rn-exchange-[a-f0-9]{32}:[A-Za-z0-9+/]{43}=", key):
            raise ValueError("invalid ephemeral cache public key")
        if info(list(expected.values()), cache_uri) != closure:
            raise ValueError(
                "cache NAR identities or references differ from the declared closure"
            )
        # Verify bytes and signatures in the SOURCE cache before touching the
        # destination store. No persistent configuration or trust is changed.
        run(
            "nix",
            "store",
            "verify",
            "--store",
            cache_uri,
            "--option",
            "require-sigs",
            "true",
            "--option",
            "trusted-public-keys",
            key,
            "--option",
            "substituters",
            "",
            *closure,
        )
        # This copies exactly the verified list, never arbitrary archive entries.
        # An untrusted daemon user fails closed; do not bypass its policy.
        run(
            "nix",
            "copy",
            "--from",
            cache_uri,
            "--no-recursive",
            "--option",
            "require-sigs",
            "true",
            "--option",
            "trusted-public-keys",
            key,
            *closure,
        )
        if info(list(expected.values())) != closure:
            raise ValueError(
                "imported closure differs from the exported NAR identities"
            )
    print(json.dumps(expected, sort_keys=True))


if __name__ == "__main__":
    try:
        main()
    except (ValueError, KeyError, OSError, subprocess.CalledProcessError) as error:
        print(f"RN exchange failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error
