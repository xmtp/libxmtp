#!/usr/bin/env python3.11
"""Build each artifact once, then render only the selected targets."""

import argparse
from contextlib import contextmanager
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import shlex
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[3]
input_spec = importlib.util.spec_from_file_location(
    "sdk_build_inputs", Path(__file__).with_name("sdk-build-inputs.py")
)
inputs = importlib.util.module_from_spec(input_spec)
input_spec.loader.exec_module(inputs)
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
                    ".cargo/config",
                    "crates/xmtp_sdk/uniffi.toml",
                )
            )
        if keep:
            selected.append((name, digest(path)))
    if not generator:
        embedded, _, _ = inputs.declared_inputs(ROOT)
        selected += list(embedded.items())
    return hashlib.sha256(json.dumps(sorted(selected)).encode()).hexdigest()


def compiler_host():
    """Read the host triple from the selected artifact compiler."""
    compiler = inputs.compiler(ROOT)
    identity = subprocess.check_output([compiler, "-vV"], cwd=ROOT).decode()
    for line in identity.splitlines():
        if line.startswith("host: "):
            return line.removeprefix("host: ")
    raise ValueError("Artifact compiler identity has no host triple")


def build_context():
    """Include the compiler and target flags in the artifact cache key."""
    compiler_path = inputs.compiler(ROOT)
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
            "PERL",
            "RANLIBFLAGS",
            "TARGET_RANLIB",
            "TARGET_RANLIBFLAGS",
            "HOST_RANLIB",
            "HOST_RANLIBFLAGS",
            "VERGEN_GIT_SHA",
            "CI",
            "XMTP_TEST_LOGGING",
            "CARGO_INCREMENTAL",
            "KACHE_ADAPTIVE_INCREMENTAL",
            "KACHE_PRESERVE_INCREMENTAL",
            "KACHE_KEY_ENV_VARS",
            "KACHE_BASE_DIR",
            "KACHE_CACHE_EXECUTABLES",
            "KACHE_BUILD_SCRIPT_CACHE",
            "KACHE_CACHE_CC_LINKS",
            "CARGO_BUILD_TARGET",
            "RUSTC_WRAPPER",
            "RUSTC_WORKSPACE_WRAPPER",
            "CARGO_BUILD_RUSTC_WRAPPER",
            "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER",
            "PROTOC",
            "PROTOC_INCLUDE",
            "LIBCLANG_PATH",
            "BINDGEN_EXTRA_CLANG_ARGS",
            "PKG_CONFIG_PATH",
            "PKG_CONFIG_LIBDIR",
            "PKG_CONFIG_SYSROOT_DIR",
            "CPATH",
            "C_INCLUDE_PATH",
            "CPLUS_INCLUDE_PATH",
            "LIBRARY_PATH",
            "LD",
            "RUSTC_BOOTSTRAP",
            "NIX_DONT_SET_RPATH",
            "NIX_ENFORCE_PURITY",
            "NIX_IGNORE_LD_THROUGH_GCC",
        )
        or name.startswith(
            (
                "CARGO_PROFILE_",
                "NIX_CFLAGS_",
                "NIX_LDFLAGS",
                "NIX_CC",
                "NIX_BINTOOLS",
                "NIX_CXXSTDLIB",
                "NIX_HARDENING",
                "PKG_CONFIG_",
                "BINDGEN_",
                "DEP_",
            )
        )
        or name.startswith(
            (
                "CARGO_TARGET_",
                "CC_",
                "CXX_",
                "CFLAGS_",
                "AR_",
                "RANLIB_",
                "RANLIBFLAGS_",
                "OPENSSL_",
            )
        )
        or "_OPENSSL_" in name
        or name.endswith("_DEPLOYMENT_TARGET")
    }
    keyed_cache_environment = {
        name: os.environ.get(name)
        for name in (
            part.strip() for part in os.environ.get("KACHE_KEY_ENV_VARS", "").split(",")
        )
        if name
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
    tool_bytes = {}
    for name, value in flags.items():
        if name in (
            "CC",
            "CXX",
            "AR",
            "LD",
            "PROTOC",
            "RUSTC_WRAPPER",
            "RUSTC_WORKSPACE_WRAPPER",
        ) or name.startswith(("CC_", "CXX_", "AR_")):
            words = shlex.split(value)
            if words:
                tool = Path(shutil.which(words[0]) or ROOT / words[0])
                tool_bytes[name] = digest(tool.resolve()) if tool.is_file() else None
    cargo, _ = inputs.cargo_config(ROOT)
    _, declared_environment, workspace_path = inputs.declared_inputs(ROOT)
    return hashlib.sha256(
        json.dumps(
            [
                compiler,
                compiler_bytes,
                flags,
                keyed_cache_environment,
                archive_indexes,
                tool_bytes,
                cargo,
                declared_environment,
                workspace_path,
            ],
            sort_keys=True,
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
    return (
        (["native"] if set(names) - {"browser"} else [])
        + ["bindgen"]
        + (["wasm", "pure"] if "browser" in names else [])
    )


def verify(record):
    for path, expected in record["files"].items():
        if not Path(path).is_file() or digest(Path(path)) != expected:
            raise ValueError(f"artifact mismatch: {path}")


def reuse(record, key, directory, expected):
    """Reuse matching role bytes from this checkout's restored cache."""
    if not record or record["key"] != key:
        return None
    for name, value in expected.items():
        if record.get(name) != value:
            raise ValueError(f"artifact context mismatch: {name}")
    names = [Path(path).name for path in record["files"]]
    if (
        not names
        or len(set(names)) != len(names)
        or any(name in ("", ".", "..") for name in names)
    ):
        raise ValueError("artifact cache file names mismatch")
    current = {
        **record,
        "files": {
            str(directory / name): checksum
            for name, checksum in zip(names, record["files"].values())
        },
    }
    verify(current)
    return current


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
    index["execution"] = []
    rust_source = source_hash()
    generator = source_hash(True)
    context = build_context()
    compiler = subprocess.check_output(
        [inputs.compiler(ROOT), "-vV"], cwd=ROOT
    ).decode()
    cacheable = inputs.cache_contract_supported(ROOT)
    compile_root = output / "build" / context
    if not cacheable:
        compile_root.mkdir(parents=True, exist_ok=True)
        compile_root = Path(tempfile.mkdtemp(prefix="unproved-", dir=compile_root))
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
                    "role-receipt-v2",
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
        expected = {
            "source": rust_source,
            "profile": profile,
            "features": features,
            "target": args.rust_target if kind == "native" else "",
            "buildContextHash": context,
            "compilerIdentity": compiler,
        }
        if kind == "bindgen":
            expected["generator"] = generator
        if cacheable and kind == "bindgen" and args.reuse_bindgen:
            shared = args.reuse_bindgen / "artifacts.json"
            if shared.exists():
                cached = reuse(
                    json.loads(shared.read_text())["artifacts"].get("bindgen"),
                    key,
                    args.reuse_bindgen.resolve() / kind,
                    expected,
                )
                if cached:
                    index["execution"].append(
                        {"role": kind, "action": "reuse-shared", "key": key}
                    )
                    index["artifacts"][kind] = cached
                    index_file.write_text(json.dumps(index, indent=2) + "\n")
                    print(f"SDK reuse shared bindgen {key}", flush=True)
                    continue
        cached = (
            reuse(index["artifacts"].get(kind), key, output / kind, expected)
            if cacheable
            else None
        )
        if cached:
            index["execution"].append({"role": kind, "action": "reuse", "key": key})
            index["artifacts"][kind] = cached
            index_file.write_text(json.dumps(index, indent=2) + "\n")
            print(f"SDK reuse {kind} {key}", flush=True)
            continue
        # Cargo writes each role in sequence. Compatible host dependencies share
        # one directory; each role still has its own receipt and saved bytes.
        cargo_target = compile_root / (
            "host" if kind in ("native", "bindgen") else "wasm"
        )
        command = [
            "cargo",
            "build",
            "--locked",
            "-p",
            "xmtp-sdk-bindgen" if kind == "bindgen" else "xmtp_sdk",
        ]
        if sys.platform != "win32":
            command.insert(0, "dev/agent-run")
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
        index["execution"].append({"role": kind, "action": "build", "key": key})
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
            "buildContextHash": context,
            "instrumentation": (
                "coverage"
                if "instrument-coverage"
                in (
                    os.environ.get("RUSTFLAGS", "")
                    + os.environ.get("CARGO_ENCODED_RUSTFLAGS", "")
                )
                else "none"
            ),
            "compilerFlags": {
                "RUSTFLAGS": inputs.tokens(os.environ.get("RUSTFLAGS", "")),
                "CARGO_ENCODED_RUSTFLAGS": inputs.tokens(
                    os.environ.get("CARGO_ENCODED_RUSTFLAGS", ""), True
                ),
            },
            "debugProfileValid": inputs.debug_profile_valid(ROOT),
            "profileOverrides": {
                name: value
                for name, value in os.environ.items()
                if name.startswith("CARGO_PROFILE_")
            },
            "compilerIdentity": compiler,
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


def render(args):
    """Regenerate the selected targets and overwrite their earlier output."""
    index = json.loads((args.artifacts / "artifacts.json").read_text())["artifacts"]
    selected = {kind: index[kind] for kind in required(args.targets)}
    # Render runs the bindgen binary and copies native bytes into the output.
    for record in selected.values():
        verify(record)
    generator = selected["bindgen"]["generator"]
    # Node and Browser staging trusts sdk-contract.json and has no later source
    # check. Mobile preflight checks it itself.
    if {"node", "browser"} & set(args.targets):
        if generator != source_hash(True):
            raise ValueError("generator mismatch; run build first")
        if any(record["source"] != source_hash() for record in selected.values()):
            raise ValueError("source mismatch; run build first")
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
    native = (
        next(
            path
            for path in selected["native"]["files"]
            if Path(path).suffix not in (".a", ".lib")
        )
        if "native" in selected
        else None
    )
    destination = args.out.resolve()
    destination.mkdir(parents=True, exist_ok=True)
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
            tree = destination / language
            if tree.exists():
                shutil.rmtree(tree)
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
