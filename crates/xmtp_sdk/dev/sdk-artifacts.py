#!/usr/bin/env python3
"""Build each artifact once, then render only the selected targets."""

import argparse
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[3]
TARGETS = ("swift", "kotlin", "node", "browser")
# Live data embedded by the SDK dependency graph through include_str!.
COMPILE_INPUTS = (
    "crates/xmtp_attachments/src/address-registry.txt",
    "crates/xmtp_id/src/scw_verifier/chain_urls_default.json",
    "crates/xmtp_id/src/scw_verifier/signature_validation.hex",
)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def source_hash(generator=False):
    if (ROOT / ".git").exists():
        paths = (
            subprocess.check_output(
                ["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard"],
                cwd=ROOT,
            )
            .decode()
            .split("\0")
        )
    else:
        paths = [
            str(path.relative_to(ROOT)) for path in ROOT.rglob("*") if path.is_file()
        ]
    selected = []
    for name in paths:
        if not name:
            continue
        path = ROOT / name
        if not path.is_file():
            continue
        if generator:
            keep = name.startswith(
                ("apps/xmtp_sdk_bindgen/", "crates/xmtp_configuration/")
            ) or name in (
                "Cargo.lock",
                "Cargo.toml",
                "crates/xmtp_sdk/uniffi.toml",
            )
        else:
            keep = (
                (
                    name.split("/")[0] in ("crates", "apps", "bindings", "proto")
                    and (
                        path.suffix in (".rs", ".proto", ".sql")
                        or path.name == "Cargo.toml"
                    )
                )
                or name in COMPILE_INPUTS
                or name
                in (
                    "Cargo.toml",
                    "Cargo.lock",
                    "flake.lock",
                    "rust-toolchain.toml",
                    ".cargo/config.toml",
                    "crates/xmtp_sdk/uniffi.toml",
                )
            )
        if keep:
            selected.append((name, digest(path)))
    return hashlib.sha256(json.dumps(sorted(selected)).encode()).hexdigest()


def build_context():
    """Include the compiler and target flags in the artifact cache key."""
    compiler_path = (
        os.environ.get("RUSTC") or os.environ.get("CARGO_BUILD_RUSTC") or "rustc"
    )
    compiler = subprocess.check_output([compiler_path, "-vV"], cwd=ROOT).decode()
    executable = Path(shutil.which(compiler_path) or ROOT / compiler_path)
    compiler_bytes = digest(executable.resolve()) if executable.is_file() else None
    flags = {
        name: value
        for name, value in os.environ.items()
        if name
        in (
            "RUSTFLAGS",
            "CARGO_ENCODED_RUSTFLAGS",
            "SDKROOT",
            "MACOSX_DEPLOYMENT_TARGET",
            "RUSTC",
            "CARGO_BUILD_RUSTC",
            "CC",
            "CXX",
            "AR",
            "RANLIB",
            "CFLAGS",
            "CXXFLAGS",
            "LDFLAGS",
        )
        or name.startswith(
            ("CARGO_TARGET_", "CC_", "CXX_", "CFLAGS_", "AR_", "RANLIB_", "OPENSSL_")
        )
        or name.endswith("_DEPLOYMENT_TARGET")
    }
    archive_indexes = {}
    for name, value in flags.items():
        if name == "RANLIB" or name.startswith("RANLIB_"):
            tool = Path(shutil.which(value) or ROOT / value)
            identity = {"path": str(tool.resolve()), "bytes": None, "version": None}
            if tool.is_file():
                identity["bytes"] = digest(tool.resolve())
                probe = subprocess.run(
                    [str(tool), "--version"], cwd=ROOT, capture_output=True, check=False
                )
                identity["version"] = [
                    probe.returncode,
                    probe.stdout.decode(errors="replace"),
                    probe.stderr.decode(errors="replace"),
                ]
            archive_indexes[name] = identity
    return hashlib.sha256(
        json.dumps(
            [compiler, compiler_bytes, flags, archive_indexes], sort_keys=True
        ).encode()
    ).hexdigest()


def run(args, **kwargs):
    print("SDK command:", " ".join(map(str, args)), flush=True)
    subprocess.run(list(map(str, args)), cwd=ROOT, check=True, **kwargs)


def targets(value):
    names = tuple(dict.fromkeys(value.split(",")))
    if not names or any(name not in TARGETS for name in names):
        raise argparse.ArgumentTypeError(
            "expected comma-separated swift,kotlin,node,browser"
        )
    return names


def required(names):
    return ["native", "bindgen"] + (["wasm", "pure"] if "browser" in names else [])


def verify(record):
    for path, expected in record["files"].items():
        if not Path(path).is_file() or digest(Path(path)) != expected:
            raise ValueError(f"artifact mismatch: {path}")


def build(args):
    if sys.platform == "darwin":
        os.environ["MACOSX_DEPLOYMENT_TARGET"] = "11.0"
        os.environ["IPHONEOS_DEPLOYMENT_TARGET"] = "14"
    output = args.artifacts.resolve()
    output.mkdir(parents=True, exist_ok=True)
    index_file = output / "artifacts.json"
    index = (
        json.loads(index_file.read_text()) if index_file.exists() else {"artifacts": {}}
    )
    rust_source = source_hash()
    generator = source_hash(True)
    context = build_context()
    for kind in required(args.targets):
        if kind == "bindgen" and args.skip_bindgen:
            continue
        profile = "debug" if kind == "bindgen" else args.profile
        features = (
            args.features
            if kind == "native"
            else ("pure-only" if kind == "pure" else "")
        )
        key = hashlib.sha256(
            json.dumps(
                [
                    kind,
                    context,
                    profile,
                    features,
                    args.rust_target if kind == "native" else "",
                    rust_source,
                    generator if kind == "bindgen" else "",
                    "vendored-static-openssl-v1" if kind == "native" else "",
                ]
            ).encode()
        ).hexdigest()
        if kind == "bindgen" and args.reuse_bindgen:
            shared = args.reuse_bindgen / "artifacts.json"
            if shared.exists():
                shared_record = json.loads(shared.read_text())["artifacts"].get(
                    "bindgen"
                )
                if shared_record and shared_record["key"] == key:
                    verify(shared_record)
                    index["artifacts"][kind] = shared_record
                    index_file.write_text(json.dumps(index, indent=2) + "\n")
                    print(f"SDK reuse shared bindgen {key}", flush=True)
                    continue
        cached = index["artifacts"].get(kind)
        if cached and cached["key"] == key:
            verify(cached)
            print(f"SDK reuse {kind} {key}", flush=True)
            continue
        cargo_target = output / "build" / kind
        command = [
            "dev/agent-run",
            "cargo",
            "build",
            "--locked",
            "-p",
            "xmtp-sdk-bindgen" if kind == "bindgen" else "xmtp_sdk",
        ]
        if profile == "release":
            command += ["--release"]
        if features:
            command += ["--features", features]
        if kind in ("wasm", "pure"):
            command += ["--target", "wasm32-unknown-unknown"]
        elif kind == "native" and args.rust_target:
            command += ["--target", args.rust_target]
        env = dict(os.environ, CARGO_TARGET_DIR=str(cargo_target))
        if kind == "native":
            # The existing vendored SQLCipher feature supplies OpenSSL. Ship
            # its static bytes in native products, without build-host dylibs.
            env["OPENSSL_NO_VENDOR"] = "0"
            env["OPENSSL_STATIC"] = "1"
        started = time.monotonic()
        run(command, env=env)
        folder = (
            cargo_target
            / (
                "wasm32-unknown-unknown"
                if kind in ("wasm", "pure")
                else args.rust_target
                if kind == "native"
                else ""
            )
            / profile
        )
        if kind == "bindgen":
            names = ["xmtp-sdk-bindgen" + (".exe" if sys.platform == "win32" else "")]
        elif kind in ("wasm", "pure"):
            names = ["xmtp_sdk.wasm"]
        else:
            platform = (
                "darwin"
                if "apple" in args.rust_target
                else "linux"
                if "android" in args.rust_target
                else sys.platform
            )
            names = (
                ["libxmtp_sdk.dylib", "libxmtp_sdk.a"]
                if platform == "darwin"
                else (
                    ["xmtp_sdk.dll", "xmtp_sdk.lib"]
                    if platform == "win32"
                    else ["libxmtp_sdk.so", "libxmtp_sdk.a"]
                )
            )
        saved = output / kind
        saved.mkdir(exist_ok=True)
        files = {}
        for name in names:
            destination = saved / name
            shutil.copy2(folder / name, destination)
            files[str(destination)] = digest(destination)
        index["artifacts"][kind] = {
            "key": key,
            "files": files,
            "seconds": time.monotonic() - started,
            "features": features,
            "profile": profile,
            "source": rust_source,
            "generator": generator,
            "target": args.rust_target if kind == "native" else "",
        }
        index_file.write_text(json.dumps(index, indent=2) + "\n")
        print(f"SDK built {kind} {key}", flush=True)


@contextmanager
def staged_output(output, prefix=".sdk-mobile-stage-"):
    """Build a sibling product and preserve the old product on failure."""
    output.parent.mkdir(parents=True, exist_ok=True)
    stage = Path(tempfile.mkdtemp(prefix=prefix, dir=output.parent))
    product = stage / "product"
    previous = stage / "previous"
    product.mkdir()
    preserve_backup = False
    try:
        yield product
        if output.exists():
            os.replace(output, previous)
        os.replace(product, output)
    except BaseException as error:
        if previous.exists():
            try:
                # A rename can finish before an interrupt reaches Python.
                if output.exists():
                    os.replace(output, product)
                os.replace(previous, output)
            except BaseException as rollback:
                preserve_backup = previous.exists()
                if preserve_backup:
                    note = f"previous product preserved at {previous}"
                    error.__notes__ = [*getattr(error, "__notes__", []), note]
                    print(note, file=sys.stderr)
                raise error from rollback
        raise
    finally:
        if not preserve_backup:
            shutil.rmtree(stage)


def target_languages(target):
    if target == "browser":
        return ("typescript-wasm", "typescript-pure")
    return ("typescript-napi" if target == "node" else target,)


def preserved_tree(tree, args, language, rust_source, generator):
    """Admit prior output without changing its recorded provenance."""
    record = json.loads((tree / "sdk-contract.json").read_text())
    artifact = record["artifact"]
    native = language not in ("typescript-wasm", "typescript-pure")
    features = (
        args.features
        if native
        else "pure-only"
        if language == "typescript-pure"
        else ""
    )
    if (
        record["generator"] != generator
        or artifact["source"] != rust_source
        or artifact["features"] != features
        or artifact["profile"] != args.profile
        or artifact["target"] != (args.rust_target if native else "")
    ):
        raise ValueError("unselected generated identity mismatch")
    artifact_files = artifact["files"]
    if not isinstance(artifact_files, dict) or not artifact_files:
        raise ValueError("unselected artifact file metadata mismatch")
    verify(artifact)
    files = record["files"]
    paths = list(tree.rglob("*"))
    if tree.is_symlink() or any(path.is_symlink() for path in paths):
        raise ValueError("unselected generated link mismatch")
    if not isinstance(files, dict):
        raise ValueError("unselected generated file metadata mismatch")
    actual = {str(path.relative_to(tree)) for path in paths if path.is_file()}
    if actual != set(files) | {"sdk-contract.json"}:
        raise ValueError("unselected generated file set mismatch")
    for name, expected in files.items():
        if digest(tree / name) != expected:
            raise ValueError("unselected generated byte mismatch")
    return record


def preserve_unselected(destination, fresh, args, rust_source, generator):
    for target in TARGETS:
        if target in args.targets:
            continue
        languages = target_languages(target)
        try:
            records = [
                preserved_tree(
                    destination / language, args, language, rust_source, generator
                )
                for language in languages
            ]
            if target == "browser" and records[0]["contract"] != records[1]["contract"]:
                raise ValueError("unselected browser contract mismatch")
        except (OSError, ValueError, KeyError, TypeError):
            continue
        for language in languages:
            shutil.copytree(destination / language, fresh / language)


def render(args):
    index = json.loads((args.artifacts / "artifacts.json").read_text())["artifacts"]
    selected = {kind: index[kind] for kind in required(args.targets)}
    generator = source_hash(True)
    source = source_hash()
    for record in selected.values():
        verify(record)
    if selected["bindgen"]["generator"] != generator:
        raise ValueError("generator contract mismatch; build matched artifacts first")
    for record in selected.values():
        if record["source"] != source:
            raise ValueError("source contract mismatch; build matched artifacts first")
    contract = hashlib.sha256(
        json.dumps(
            {
                kind: {
                    "files": {
                        Path(path).name: checksum
                        for path, checksum in record["files"].items()
                    },
                    "generator": generator,
                }
                for kind, record in selected.items()
            },
            sort_keys=True,
        ).encode()
    ).hexdigest()
    binary = next(iter(selected["bindgen"]["files"]))
    native = next(
        path
        for path in selected["native"]["files"]
        if Path(path).suffix != ".a" and Path(path).suffix != ".lib"
    )
    destination = args.out.resolve()
    destination.parent.mkdir(parents=True, exist_ok=True)
    with staged_output(destination, prefix=".sdk-render-stage-") as fresh:
        preserve_unselected(destination, fresh, args, source, generator)
        for target in args.targets:
            for language in target_languages(target):
                artifact = (
                    "pure"
                    if language == "typescript-pure"
                    else "wasm"
                    if language == "typescript-wasm"
                    else "native"
                )
                library = (
                    next(iter(selected[artifact]["files"]))
                    if artifact != "native"
                    else native
                )
                tree = fresh / language
                command = [
                    binary,
                    "generate",
                    "--lib",
                    library,
                    "--language",
                    "typescript-wasm" if language == "typescript-pure" else language,
                    "--out",
                    tree,
                    "--config",
                    "apps/xmtp_sdk_bindgen/uniffi-global.toml",
                ]
                if language == "typescript-pure":
                    command += ["--pure-only"]
                if args.no_format:
                    command += ["--no-format"]
                run(command)
                if artifact in ("wasm", "pure"):
                    run([binary, "stage-wasm", "--lib", library, "--out", tree])
                elif language == "typescript-napi":
                    shutil.copy2(native, tree / Path(native).name)
                files = {
                    str(path.relative_to(tree)): digest(path)
                    for path in sorted(tree.rglob("*"))
                    if path.is_file()
                }
                (tree / "sdk-contract.json").write_text(
                    json.dumps(
                        {
                            "contract": contract,
                            "generator": generator,
                            "artifact": selected[artifact],
                            "files": files,
                        },
                        indent=2,
                    )
                    + "\n"
                )
    print(f"SDK rendered {','.join(args.targets)} contract {contract}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("build", "render"))
    parser.add_argument("--targets", type=targets, default=TARGETS)
    parser.add_argument("--artifacts", type=Path, default=ROOT / "target/sdk-artifacts")
    parser.add_argument(
        "--out",
        type=Path,
        default=Path(
            os.environ.get("XMTP_SDK_GENERATED_DIR", str(ROOT / "target/sdk-generated"))
        ),
    )
    parser.add_argument("--profile", choices=("debug", "release"), default="debug")
    parser.add_argument("--features", default="")
    parser.add_argument("--rust-target", default="")
    parser.add_argument("--skip-bindgen", action="store_true")
    parser.add_argument(
        "--reuse-bindgen", type=Path, default=ROOT / "target/sdk-artifacts"
    )
    parser.add_argument("--no-format", action="store_true")
    args = parser.parse_args()
    try:
        (build if args.action == "build" else render)(args)
    except (ValueError, KeyError, OSError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"SDK artifact error: {error}\n")


if __name__ == "__main__":
    main()
