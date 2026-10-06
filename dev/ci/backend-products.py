#!/usr/bin/env python3
"""Transport a Linux backend image and its native Nix runtime closure."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile

FAMILY = "backend-linux-x64"
IMAGE = "ghcr.io/xmtp/backend:self-hosted"
PAYLOAD = {"manifest.json", "image.tar", "runtime.export"}
STORE_PATH = re.compile(r"/nix/store/[a-z0-9]{32}-[^/\s]+$")


def run(args, **kwargs):
    if args[0] in {"nix", "nix-store"} and "env" not in kwargs:
        environment = dict(os.environ)
        environment.pop("LD_LIBRARY_PATH", None)
        kwargs["env"] = environment
    return subprocess.run(args, check=True, **kwargs)


def output(args):
    return run(args, stdout=subprocess.PIPE, text=True).stdout.strip()


def digest(path):
    value = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def hash_paths(root, paths):
    value = hashlib.sha256()
    for name in sorted(paths):
        path = root / name
        value.update(name.encode() + b"\0")
        # Git tracks a link's target text, not files outside the checkout.
        data = os.readlink(path).encode() if path.is_symlink() else path.read_bytes()
        value.update(str(path.lstat().st_mode & 0o777).encode() + b"\0")
        value.update(data + b"\0")
    return value.hexdigest()


def identity(root):
    paths = output(["git", "-C", str(root), "ls-files", "-z"]).split("\0")
    paths = [p for p in paths if p]
    sha = output(["git", "-C", str(root), "rev-parse", "HEAD"])
    if os.environ.get("GITHUB_SHA", sha) != sha:
        raise ValueError("checkout SHA differs from GITHUB_SHA")
    build_paths = [
        p
        for p in paths
        if p.startswith(("nix/", ".cargo/"))
        or p
        in {
            "flake.nix",
            "flake.lock",
            "Cargo.toml",
            "Cargo.lock",
            "rust-toolchain.toml",
        }
    ]
    generator_paths = [
        p
        for p in paths
        if p.startswith(
            ("dev/ci/backend-products", "dev/backend/", "dev/docker/", "nix/")
        )
        or p in {"flake.nix", "flake.lock"}
    ]
    compiler_paths = [
        p
        for p in paths
        if p in {"flake.nix", "flake.lock", "rust-toolchain.toml"}
        or p == "nix/lib/mkToolchain.nix"
    ]
    locks = {p: digest(root / p) for p in ("Cargo.lock", "flake.lock")}
    return {
        "schemaVersion": 1,
        "family": FAMILY,
        "runId": int(os.environ["GITHUB_RUN_ID"]),
        "runAttempt": int(os.environ["GITHUB_RUN_ATTEMPT"]),
        "checkoutSha": sha,
        "sourceHash": hash_paths(root, paths),
        "generatorHash": hash_paths(root, generator_paths),
        "context": {
            "os": "linux",
            "arch": "x64",
            "host": "x86_64-unknown-linux-gnu",
            "target": "x86_64-unknown-linux-gnu",
            "profile": "release",
            "compilerIdentity": hash_paths(root, compiler_paths),
            "buildContextHash": hash_paths(root, build_paths),
            "instrumentation": "none",
            "features": [],
            "dependencyLocks": locks,
            "imageTarget": "x86_64-unknown-linux-musl",
        },
        "testInventory": [],
    }


def check_identity(root, manifest, earlier):
    expected = identity(root)
    if expected["runId"] <= 0 or expected["runAttempt"] <= 0:
        raise ValueError("run identity must be positive")
    if earlier:
        attempt = manifest.get("runAttempt")
        if type(attempt) is not int or not 0 < attempt <= expected["runAttempt"]:
            raise ValueError("invalid producer runAttempt")
        expected["runAttempt"] = attempt
    for key, value in expected.items():
        if type(manifest.get(key)) is not type(value) or manifest.get(key) != value:
            raise ValueError(f"product {key} differs from current checkout or run")


def linux_host():
    if platform.system() != "Linux" or platform.machine() not in ("x86_64", "AMD64"):
        raise ValueError("backend products require a Linux x64 host")


def closure_info(native):
    records = json.loads(output(["nix", "path-info", "--recursive", "--json", native]))
    if isinstance(records, dict):
        records = [dict(record, path=path) for path, record in records.items()]
    result = {}
    for record in records:
        path = record["path"]
        if not STORE_PATH.fullmatch(path):
            raise ValueError("invalid Nix runtime path")
        result[path] = record["narHash"]
    if native not in result:
        raise ValueError("native output missing from runtime closure")
    return dict(sorted(result.items()))


def image_id(path):
    with tarfile.open(path) as archive:
        member = archive.getmember("manifest.json")
        if not member.isfile():
            raise ValueError("invalid image manifest")
        records = json.load(archive.extractfile(member))
        if len(records) != 1 or records[0].get("RepoTags") != [IMAGE]:
            raise ValueError("image must contain only the backend tag")
        config = archive.getmember(records[0]["Config"])
        if not config.isfile():
            raise ValueError("invalid image configuration")
        data = archive.extractfile(config).read()
        settings = json.loads(data)
        if settings.get("architecture") != "amd64" or settings.get("os") != "linux":
            raise ValueError("image must target Linux amd64")
        return "sha256:" + hashlib.sha256(data).hexdigest()


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def export_product(root, native, image, destination, expected=None):
    linux_host()
    manifest = identity(root)
    if expected is not None and manifest != expected:
        raise ValueError("checkout changed during product build")
    check_identity(root, manifest, False)
    native = str(Path(native).resolve())
    if (
        not STORE_PATH.fullmatch(native)
        or not (Path(native) / "bin/xmtp-backend").is_file()
    ):
        raise ValueError("native output must contain bin/xmtp-backend")
    manifest_identity = dict(manifest)
    closure = closure_info(native)
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=destination.parent) as temp:
        stage = Path(temp)
        shutil.copy2(image, stage / "image.tar")
        with (stage / "runtime.export").open("wb") as stream:
            run(["nix-store", "--export", *closure], stdout=stream)
        manifest["runtimes"] = {
            "nativeOutput": native,
            "closure": closure,
            "imageId": image_id(stage / "image.tar"),
            "imageTag": IMAGE,
            "binaryHash": digest(Path(native) / "bin/xmtp-backend"),
        }
        manifest["files"] = {
            name: digest(stage / name) for name in ("image.tar", "runtime.export")
        }
        manifest["modes"] = {
            name: (stage / name).stat().st_mode & 0o777 for name in manifest["files"]
        }
        write_json(stage / "manifest.json", manifest)
        temporary = destination.with_suffix(destination.suffix + ".tmp")
        with tarfile.open(temporary, "w") as archive:
            for name in sorted(PAYLOAD):
                archive.add(stage / name, arcname=name, recursive=False)
        if identity(root) != manifest_identity:
            raise ValueError("checkout changed during product export")
        temporary.replace(destination)
        shutil.copyfile(
            stage / "manifest.json", destination.with_suffix(".manifest.json")
        )


def validate_payload(root, stage, earlier):
    manifest = json.loads((stage / "manifest.json").read_text())
    check_identity(root, manifest, earlier)
    if set(manifest.get("files", {})) != PAYLOAD - {"manifest.json"}:
        raise ValueError("incomplete product file inventory")
    if set(manifest.get("modes", {})) != set(manifest["files"]):
        raise ValueError("incomplete product mode inventory")
    for name, value in manifest["files"].items():
        path = stage / name
        if not path.is_file() or path.is_symlink() or digest(path) != value:
            raise ValueError(f"product bytes differ: {name}")
        if path.stat().st_mode & 0o777 != manifest["modes"][name]:
            raise ValueError(f"product mode differs: {name}")
    runtime = manifest["runtimes"]
    if runtime.get("imageTag") != IMAGE or runtime.get("imageId") != image_id(
        stage / "image.tar"
    ):
        raise ValueError("product image identity differs")
    native = runtime["nativeOutput"]
    if not STORE_PATH.fullmatch(native) or native not in runtime["closure"]:
        raise ValueError("invalid native runtime identity")
    if not runtime["closure"] or any(
        not STORE_PATH.fullmatch(p) for p in runtime["closure"]
    ):
        raise ValueError("invalid closure path inventory")
    return manifest


def check_runtime(manifest):
    runtime = manifest["runtimes"]
    if closure_info(runtime["nativeOutput"]) != runtime["closure"]:
        raise ValueError("runtime closure differs or is incomplete")
    run(["nix-store", "--verify-path", *runtime["closure"]], stdout=sys.stderr)
    binary = Path(runtime["nativeOutput"]) / "bin/xmtp-backend"
    if not binary.is_file() or not os.access(binary, os.X_OK):
        raise ValueError("native backend is missing or is not executable")
    if digest(binary) != runtime["binaryHash"]:
        raise ValueError("native backend bytes differ")
    return binary


def loaded_image(manifest):
    loaded = output(["docker", "image", "inspect", "--format", "{{.Id}}", IMAGE])
    if loaded != manifest["runtimes"]["imageId"]:
        raise ValueError("loaded backend image differs from prepared product")


def restore(root, source, earlier, load):
    linux_host()
    destination = root / "target/ci-products" / FAMILY
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=destination.parent) as temp:
        stage = Path(temp)
        with tarfile.open(source) as archive:
            members = archive.getmembers()
            if len(members) != len(PAYLOAD) or {m.name for m in members} != PAYLOAD:
                raise ValueError(
                    "archive contains missing, duplicate, or unexpected paths"
                )
            for member in members:
                if not member.isfile() or member.mode & 0o7000:
                    raise ValueError("archive contains an unsafe member")
                with (stage / member.name).open("wb") as stream:
                    shutil.copyfileobj(archive.extractfile(member), stream)
                (stage / member.name).chmod(member.mode)
        manifest = validate_payload(root, stage, earlier)
        with (stage / "runtime.export").open("rb") as stream:
            run(["nix-store", "--import"], stdin=stream, stdout=sys.stderr)
        binary = check_runtime(manifest)
        if load:
            run(
                ["docker", "load", "--input", str(stage / "image.tar")],
                stdout=sys.stderr,
            )
            loaded_image(manifest)
        write_json(
            stage / "prepared.json",
            {
                "checkoutRoot": str(root),
                "manifestHash": digest(stage / "manifest.json"),
                "allowEarlierAttempt": earlier,
            },
        )
        if destination.exists():
            shutil.rmtree(destination)
        shutil.copytree(stage, destination)
        run(
            [
                "nix-store",
                "--add-root",
                str(destination / "native-runtime"),
                "--realise",
                manifest["runtimes"]["nativeOutput"],
            ],
            stdout=sys.stderr,
        )
    return binary


def verify_prepared(root, path):
    linux_host()
    expected_path = root / "target/ci-products" / FAMILY / "manifest.json"
    if path.resolve() != expected_path:
        raise ValueError("prepared manifest belongs to another checkout")
    prepared = json.loads(path.with_name("prepared.json").read_text())
    if prepared["checkoutRoot"] != str(root) or prepared["manifestHash"] != digest(
        path
    ):
        raise ValueError(
            "prepared product belongs to another checkout or changed manifest"
        )
    manifest = validate_payload(root, path.parent, prepared["allowEarlierAttempt"])
    binary = check_runtime(manifest)
    loaded_image(manifest)
    return binary


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root", type=Path, default=Path(__file__).resolve().parents[2]
    )
    commands = parser.add_subparsers(dest="command", required=True)
    build = commands.add_parser("build")
    build.add_argument("--output", type=Path, required=True)
    export = commands.add_parser("export")
    export.add_argument("--native-output", required=True)
    export.add_argument("--image-input", type=Path, required=True)
    export.add_argument("--output", type=Path, required=True)
    restore_cmd = commands.add_parser("restore")
    restore_cmd.add_argument("--input", type=Path, required=True)
    restore_cmd.add_argument("--allow-earlier-attempt", action="store_true")
    restore_cmd.add_argument("--load-image", action="store_true")
    verify = commands.add_parser("verify-prepared")
    verify.add_argument("--manifest", type=Path, required=True)
    args = parser.parse_args()
    root = args.root.resolve()
    if args.command == "build":
        linux_host()
        expected = identity(root)
        environment = dict(os.environ)
        environment.pop("LD_LIBRARY_PATH", None)
        result = run(
            [
                "nix",
                "build",
                "--no-link",
                "--json",
                str(root) + "#backend-image-x86_64-unknown-linux-musl",
                str(root) + "#xmtp-backend",
            ],
            stdout=subprocess.PIPE,
            text=True,
            env=environment,
        )
        products = json.loads(result.stdout)
        outputs = [record["outputs"]["out"] for record in products]
        native = next(p for p in outputs if (Path(p) / "bin/xmtp-backend").is_file())
        image = next(p for p in outputs if Path(p).is_file())
        export_product(root, native, image, args.output.resolve(), expected)
    elif args.command == "export":
        linux_host()
        for output_name, supplied in (
            ("xmtp-backend", Path(args.native_output)),
            ("backend-image-x86_64-unknown-linux-musl", args.image_input),
        ):
            current = output(
                ["nix", "eval", "--raw", str(root) + "#" + output_name + ".outPath"]
            )
            if supplied.resolve() != Path(current):
                raise ValueError("export output differs from current Nix inputs")
        export_product(
            root, args.native_output, args.image_input, args.output.resolve()
        )
    elif args.command == "restore":
        print(restore(root, args.input, args.allow_earlier_attempt, args.load_image))
    else:
        print(verify_prepared(root, args.manifest))


if __name__ == "__main__":
    try:
        main()
    except (
        ValueError,
        KeyError,
        OSError,
        subprocess.CalledProcessError,
        tarfile.TarError,
        StopIteration,
    ) as error:
        print(f"backend-products: {error}", file=sys.stderr)
        sys.exit(1)
