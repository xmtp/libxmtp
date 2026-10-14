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


# The file types of the SDK and bindgen closures (`nix/lib/sdk-sources.nix`):
# Cargo sources and SQL, compiled-in data, and the render runtimes and
# configuration. Other untracked files, such as proptest regression seeds or
# merge leftovers, are not inputs.
INPUT_SUFFIXES = (
    b".rs",
    b".toml",
    b".lock",
    b".sql",
    b".proto",
    b".json",
    b".hex",
    b".txt",
    b".swift",
    b".kt",
    b".ts",
    b".js",
)


def untracked_inputs():
    """Untracked, unignored SDK input files under the SDK input paths."""
    names = subprocess.check_output(
        ["git", "ls-files", "-z", "--others", "--exclude-standard", "--"]
        + list(COMMIT_INPUTS),
        cwd=ROOT,
    ).split(b"\0")
    return sorted(
        name
        for name in names
        if name.endswith(INPUT_SUFFIXES) and b"/proptest-regressions/" not in name
    )


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
        return {
            "source": identity,
            "generator": identity,
            "render": dict.fromkeys(("swift", "kotlin", "typescript"), identity),
        }
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


def fork_revision():
    """The pinned uniffi-bindgen-react-native fork revision."""
    # Imported here: other actions also run on Python before 3.11.
    import tomllib

    manifest = tomllib.loads((ROOT / "Cargo.toml").read_text())
    return manifest["workspace"]["metadata"]["xmtp-sdk-fork"]["rev"]


def source_key(profile="", rust_target=""):
    """The source identity of a cargo-built pair (the Windows Node pair).

    It digests the committed trees of the SDK input paths, so two commits
    with the same SDK inputs share it, and it names no commit. With a Rust
    target it adds `key`, the pair's cache key: source, profile, target, and
    the runtime fork revision. The toolchain is part of the source, through
    `rust-toolchain.toml`.
    """
    changed = subprocess.run(
        ["git", "diff", "--quiet", "HEAD", "--", *COMMIT_INPUTS], cwd=ROOT
    ).returncode
    if changed or untracked_inputs():
        raise ValueError("the source key needs committed SDK inputs")
    trees = subprocess.check_output(
        ["git", "ls-tree", "HEAD", "--", *COMMIT_INPUTS], cwd=ROOT
    )
    identity = {
        "source": "tree:" + hashlib.sha256(trees).hexdigest(),
        "forkRevision": fork_revision(),
    }
    if rust_target:
        identity["key"] = hashlib.sha256(
            json.dumps(
                [
                    "native",
                    profile,
                    "",
                    rust_target,
                    identity["source"],
                    "vendored-static-openssl-v1",
                    identity["forkRevision"],
                ]
            ).encode()
        ).hexdigest()
    return identity


def source_hash(generator=False):
    """The SDK source identity, or with `generator` the bindgen executable's."""
    return provenance()["generator" if generator else "source"]


def render_hash(language):
    """The identity a generated tree records: the bindgen plus its language's
    runtime and configuration, so a runtime edit changes only that language."""
    return provenance()["render"][
        "typescript" if language.startswith("typescript-") else language
    ]


# The branch whose commits the warm workflow builds into the trusted cache.
TRUNK = "origin/self-hosted"
# Files outside the crates that change every Cargo build of a package.
CARGO_INPUTS = ("Cargo.toml", "Cargo.lock", "rust-toolchain.toml", ".cargo")


def closure_paths(package):
    """The directories of the workspace crates that `package` builds with.

    Normal and build dependencies only, from `cargo metadata`; each path is
    relative to the repository root.
    """
    metadata = json.loads(
        subprocess.check_output(
            ["cargo", "metadata", "--format-version", "1", "--locked", "--offline"],
            cwd=ROOT,
        )
    )
    local = {
        item["id"]: Path(item["manifest_path"]).parent
        for item in metadata["packages"]
        if item["source"] is None
    }
    nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
    start = next(item["id"] for item in metadata["packages"] if item["name"] == package)
    pending, seen = [start], {start}
    while pending:
        for dependency in nodes[pending.pop()]["deps"]:
            built = any(kind["kind"] != "dev" for kind in dependency["dep_kinds"])
            if built and dependency["pkg"] in local and dependency["pkg"] not in seen:
                seen.add(dependency["pkg"])
                pending.append(dependency["pkg"])
    return sorted(str(local[item].relative_to(ROOT)) for item in seen)


def bindgen_route():
    """Return where the bindgen comes from: `nix` or `cargo`.

    `nix` (`.#xmtp-sdk-bindgen`, a cache hit when trunk built it) unless a
    file of the bindgen's crates or Cargo inputs differs from the merge base
    with trunk: then a cargo debug build, which an edit rebuilds faster than
    Nix. CI, a missing merge base (a shallow clone), or a failed check keep
    cargo. XMTP_SDK_BINDGEN=nix|cargo overrides the choice.
    """
    choice = os.environ.get("XMTP_SDK_BINDGEN", "")
    if choice in ("nix", "cargo"):
        return choice
    if choice:
        raise ValueError(f"XMTP_SDK_BINDGEN must be nix or cargo, not {choice}")
    if os.environ.get("CI"):
        return "cargo"
    try:
        base = subprocess.check_output(
            ["git", "merge-base", "HEAD", TRUNK],
            cwd=ROOT,
            text=True,
            stderr=subprocess.DEVNULL,
        ).strip()
        paths = [*closure_paths("xmtp-sdk-bindgen"), *CARGO_INPUTS]
        changed = subprocess.run(
            ["git", "diff", "--quiet", base, "--", *paths], cwd=ROOT
        ).returncode
        added = subprocess.check_output(
            ["git", "ls-files", "--others", "--exclude-standard", "--", *paths],
            cwd=ROOT,
        )
    except (subprocess.CalledProcessError, OSError, StopIteration):
        return "cargo"
    return "cargo" if changed or added else "nix"


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
    return hashlib.sha256(
        json.dumps([compiler, compiler_bytes, flags], sort_keys=True).encode()
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
    # No browser artifact links the host library.
    native = ["native"] if set(names) - {"browser"} else []
    return native + ["bindgen"] + (["wasm", "pure"] if "browser" in names else [])


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
        if kind == "bindgen" and bindgen_route() == "nix":
            link = output / "bindgen-nix"
            run(["nix", "build", "--out-link", link, ".#xmtp-sdk-bindgen"])
            binary = link / "bin/xmtp-sdk-bindgen"
            index["artifacts"][kind] = {
                "key": f"nix:{binary.resolve()}",
                "files": {str(binary): digest(binary)},
                "seconds": 0,
                "features": "",
                "profile": "release",
                "source": rust_source,
                "generator": generator,
                "target": "",
            }
            index_file.write_text(json.dumps(index, indent=2) + "\n")
            print("SDK bindgen from Nix", flush=True)
            continue
        profile = "debug" if kind == "bindgen" else args.profile
        features = "pure-only" if kind == "pure" else ""
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
        cached = index["artifacts"].get(kind)
        if cached and cached["key"] == key:
            verify(cached)
            print(f"SDK reuse {kind} {key}", flush=True)
            continue
        # The two WASM builds share one target directory and its dependencies.
        cargo_target = output / "build" / ("wasm" if kind == "pure" else kind)
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
    """Build the pinned @ubjs runtimes and return each package directory.

    Each runtime gets an out-link under target/sdk-runtimes, a GC root, and a
    tree links its package through that out-link, so a garbage collection
    keeps the links working.
    """
    packages = {}
    for name in names:
        link = ROOT / "target/sdk-runtimes" / name
        subprocess.run(
            ["nix", "build", "--out-link", str(link), f".#ubjs-{name}"],
            cwd=ROOT,
            check=True,
        )
        packages[name] = link / "lib/node_modules/@ubjs" / name
    return packages


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
    # Each tree records its render identity from this checkout, so the bindgen
    # that renders it must be this checkout's.
    if generator != source_hash(True):
        raise ValueError("generator mismatch; run build first")
    # Node and Browser staging trusts sdk-contract.json and has no later source
    # check. Mobile preflight checks it itself.
    if {"node", "browser"} & set(args.targets):
        if any(record["source"] != source_hash() for record in selected.values()):
            raise ValueError("source mismatch; run build first")
    binary = next(iter(selected["bindgen"]["files"]))
    native = next(
        (
            path
            for path in selected.get("native", {"files": {}})["files"]
            if Path(path).suffix not in (".a", ".lib")
        ),
        None,
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
                    render_hash(language),
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
    # `provenance` prints the source and Node render identities, for a caller
    # that runs `xmtp-sdk-bindgen render` itself (the Windows load job). `key`
    # prints the source key of a cargo-built pair as JSON (`source_key`).
    # `paths PACKAGE` prints the workspace crates that PACKAGE builds with.
    parser.add_argument(
        "action", choices=("build", "render", "provenance", "key", "paths")
    )
    parser.add_argument("package", nargs="?", default="xmtp_sdk")
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
    parser.add_argument("--rust-target", default="")
    parser.add_argument("--skip-bindgen", action="store_true")
    args = parser.parse_args()
    try:
        if args.action == "provenance":
            # The identities of a Node render.
            print(source_hash(), render_hash("typescript-napi"))
        elif args.action == "key":
            print(json.dumps(source_key(args.profile, args.rust_target)))
        elif args.action == "paths":
            print("\n".join(closure_paths(args.package)))
        else:
            (build if args.action == "build" else render)(args)
    except (ValueError, KeyError, OSError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"SDK artifact error: {error}\n")


if __name__ == "__main__":
    main()
