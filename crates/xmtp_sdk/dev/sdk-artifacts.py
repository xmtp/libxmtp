#!/usr/bin/env python3
"""Build each artifact once, then render only the selected targets."""

import argparse
from contextlib import contextmanager
import functools
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
# Paths that contain every input of the SDK and bindgen closures. Windows has
# no Nix, so its identity covers these paths instead of the closures; on other
# systems, an untracked file here is an input that Nix cannot see.
COMMIT_INPUTS = (
    "Cargo.toml",
    "Cargo.lock",
    ".cargo",
    "rust-toolchain.toml",
    "crates",
    "proto",
    "apps/xmtp_sdk_bindgen",
)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def untracked_inputs():
    """Untracked, unignored files under the SDK input paths."""
    names = subprocess.check_output(
        ["git", "ls-files", "-z", "--others", "--exclude-standard", "--"]
        + list(COMMIT_INPUTS),
        cwd=ROOT,
    ).split(b"\0")
    return sorted(filter(None, names))


@functools.cache
def provenance():
    """Return the SDK closure identities that Nix computes at evaluation.

    `nix/lib/sdk-sources.nix` defines them, so the Nix and cargo routes record
    the same values. Nix evaluates the git checkout and skips untracked files,
    so an untracked input file is an error until it is added. Windows has no
    Nix; its builds name the
    commit, plus a digest of uncommitted changes to the SDK inputs, so a
    working-tree edit never reuses a build of the commit.
    """
    if sys.platform == "win32":
        commit = subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True
        ).strip()
        identity = f"commit:{commit}"
        changes = subprocess.check_output(
            ["git", "diff", "HEAD", "--binary", "--", *COMMIT_INPUTS], cwd=ROOT
        )
        untracked = untracked_inputs()
        if changes or untracked:
            state = hashlib.sha256(changes)
            for name in untracked:
                state.update(b"\0" + name + b"\0" + (ROOT / name.decode()).read_bytes())
            identity += f"+dirty:{state.hexdigest()}"
        return {"source": identity, "generator": identity}
    untracked = untracked_inputs()
    if untracked:
        raise ValueError(
            "the Nix SDK identity skips untracked files; `git add` them: "
            + ", ".join(name.decode() for name in untracked)
        )
    return json.loads(
        subprocess.check_output(
            ["nix", "eval", "--json", f"{ROOT}#sdkProvenance"], cwd=ROOT
        )
    )


def source_hash(generator=False):
    return provenance()["generator" if generator else "source"]


def compiler_host():
    """Read the host triple from the selected artifact compiler."""
    compiler = os.environ.get("RUSTC") or os.environ.get("CARGO_BUILD_RUSTC") or "rustc"
    identity = subprocess.check_output([compiler, "-vV"], cwd=ROOT).decode()
    for line in identity.splitlines():
        if line.startswith("host: "):
            return line.removeprefix("host: ")
    raise ValueError("Artifact compiler identity has no host triple")


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
            "PERL",
            "RANLIBFLAGS",
            "TARGET_RANLIB",
            "TARGET_RANLIBFLAGS",
            "HOST_RANLIB",
            "HOST_RANLIBFLAGS",
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


# The `render --target` of each generated language.
RENDER_TARGETS = {
    "swift": "swift",
    "kotlin": "kotlin",
    "typescript-napi": "node",
    "typescript-wasm": "browser-worker",
    "typescript-pure": "browser-pure",
}


# The @ubjs runtimes that each TypeScript tree imports (`nix/package/xmtp-sdk.nix`).
RUNTIMES = {
    "typescript-napi": ("core", "node"),
    "typescript-wasm": ("core", "wasm"),
    "typescript-pure": ("core", "wasm"),
}


def runtime_packages(names):
    """Build the pinned @ubjs runtimes and return each package directory."""
    built = json.loads(
        subprocess.check_output(
            [
                "nix",
                "build",
                "--no-link",
                "--json",
                *(f".#ubjs-{name}" for name in names),
            ],
            cwd=ROOT,
        )
    )
    outputs = [Path(result["outputs"]["out"]) for result in built]
    return {
        name: next(path for path in outputs if f"-ubjs-{name}-" in path.name)
        / "lib/node_modules/@ubjs"
        / name
        for name in names
    }


def target_languages(target):
    if target == "browser":
        return ("typescript-wasm", "typescript-pure")
    return ("typescript-napi" if target == "node" else target,)


def formatter():
    """Return the `--oxfmt` arguments for a TypeScript render.

    The bindgen reads `XMTP_SDK_OXFMT` itself; the `default` shell exports it.
    Otherwise (the `rust` shell) use the same nixpkgs oxfmt that the Nix render
    uses, from this checkout's `flake.lock`.
    """
    if os.environ.get("XMTP_SDK_OXFMT"):
        return []
    store = subprocess.check_output(
        [
            "nix",
            "build",
            "--no-link",
            "--print-out-paths",
            "--inputs-from",
            ".",
            "nixpkgs#oxfmt",
        ],
        cwd=ROOT,
        text=True,
    ).strip()
    return ["--oxfmt", f"{store}/bin/oxfmt"]


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
    binary = next(iter(selected["bindgen"]["files"]))
    native = next(
        path
        for path in selected["native"]["files"]
        if Path(path).suffix != ".a" and Path(path).suffix != ".lib"
    )
    typescript = {"node", "browser"} & set(args.targets)
    oxfmt = formatter() if typescript else []
    runtimes = (
        runtime_packages(
            sorted(
                {
                    name
                    for target in args.targets
                    for language in target_languages(target)
                    for name in RUNTIMES.get(language, ())
                }
            )
        )
        if typescript
        else {}
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
            record = selected[artifact]
            library = next(iter(record["files"])) if artifact != "native" else native
            tree = destination / language
            if tree.exists():
                shutil.rmtree(tree)
            # The bindgen writes the tree and its receipt, sdk-contract.json.
            run(
                [
                    binary,
                    "render",
                    "--lib",
                    library,
                    "--target",
                    RENDER_TARGETS[language],
                    "--out",
                    tree,
                    "--source",
                    record["source"],
                    "--generator",
                    generator,
                    "--profile",
                    record["profile"],
                    "--features",
                    record["features"],
                    "--rust-target",
                    record["target"],
                    *(oxfmt if language.startswith("typescript-") else []),
                ]
            )
            # Like the Nix render, the tree loads its runtimes from node_modules.
            for name in RUNTIMES.get(language, ()):
                link = tree / "node_modules/@ubjs" / name
                link.parent.mkdir(parents=True, exist_ok=True)
                link.symlink_to(runtimes[name])
    print(f"SDK rendered {','.join(args.targets)} with generator {generator}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    # `provenance` prints the source and generator identities, for a caller
    # that runs `xmtp-sdk-bindgen render` itself (the Windows load job).
    parser.add_argument("action", choices=("build", "render", "provenance"))
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
    args = parser.parse_args()
    try:
        if args.action == "provenance":
            print(source_hash(), source_hash(True))
        else:
            (build if args.action == "build" else render)(args)
    except (ValueError, KeyError, OSError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"SDK artifact error: {error}\n")


if __name__ == "__main__":
    main()
