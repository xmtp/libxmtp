#!/usr/bin/env python3
"""Record a cold Linux SDK build in the production Rust shell."""

import argparse
import json
import os
from pathlib import Path
import platform
import shlex
import shutil
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[2]
ROLES = {"node": {"native", "bindgen"}, "browser": {"bindgen", "wasm", "pure"}}
TREES = {
    "node": {"swift", "kotlin", "typescript-napi"},
    "browser": {"typescript-wasm", "typescript-pure"},
}


def read(path):
    return json.loads(path.read_text())


def write(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def outside_checkout(directory):
    directory = directory.resolve()
    if directory.is_relative_to(ROOT.resolve()) or "\n" in str(directory):
        raise ValueError("proof directory must be outside the checkout")
    return directory


def no_xcode():
    record = {
        "system": platform.system(),
        "machine": platform.machine(),
        "xcodeAppExists": Path("/Applications/Xcode.app").exists(),
        "xcodebuild": shutil.which("xcodebuild"),
        "xcrun": shutil.which("xcrun"),
    }
    if record["system"] != "Linux" or any(
        record[key] for key in ("xcodeAppExists", "xcodebuild", "xcrun")
    ):
        raise ValueError("proof requires Linux without Xcode commands or application")
    return record


def create_guard(directory, real_nix):
    """Keep the guard first when Nix replaces PATH for a child shell."""
    directory.mkdir(parents=True, exist_ok=True)
    binary = directory / "bin"
    binary.mkdir(exist_ok=True)
    shutil.copyfile(__file__, directory / "proof.py")
    write(directory / "guard.json", {"realNix": str(Path(real_nix).resolve())})
    command = " ".join(
        shlex.quote(str(part))
        for part in (
            sys.executable,
            directory / "proof.py",
            "guard",
            "--directory",
            directory,
            "--",
        )
    )
    wrapper = binary / "nix"
    wrapper.write_text("#!/bin/sh\nexec " + command + ' "$@"\n')
    wrapper.chmod(0o755)
    # Do not reload a Nix environment or change any other compiler variable.
    (directory / "bash-env").write_text(
        "export PATH=" + shlex.quote(str(binary)) + ':"$PATH"\n'
    )
    return binary


def guard(directory, arguments):
    arguments = arguments[1:] if arguments[:1] == ["--"] else arguments
    shell = None
    allowed = True
    command = next(
        (
            name
            for name in ("develop", "build", "eval", "store", "path-info")
            if name in arguments
        ),
        "other",
    )
    if command == "develop":
        tail = arguments[arguments.index("develop") + 1 :]
        selector = tail[0] if tail and not tail[0].startswith("-") else ""
        attribute = selector.split("#", 1)[1] if "#" in selector else "implicit"
        shell = (
            attribute
            if attribute in {"rust", "js", "js-node", "default", "local", "implicit"}
            else "other"
        )
        allowed = shell in {"rust", "js", "js-node"}
    phase = os.environ.get("SDK_RUST_PROOF_PHASE", "run")
    phase = phase if phase in {"probe", "identity"} else "run"
    event = {"command": command, "shell": shell, "allowed": allowed, "phase": phase}
    descriptor = os.open(
        directory / "nix-calls.jsonl", os.O_WRONLY | os.O_CREAT | os.O_APPEND, 0o600
    )
    try:
        os.write(descriptor, (json.dumps(event) + "\n").encode())
    finally:
        os.close(descriptor)
    if not allowed:
        print(f"SDK_RUST_SHELL_PROOF_BLOCKED shell={shell}", file=sys.stderr)
        return 2
    real_nix = read(directory / "guard.json")["realNix"]
    os.execv(real_nix, [real_nix, *arguments])


def identity(directory):
    absence = no_xcode()
    wrapper_id = os.environ.get("XMTP_NIX_WRAPPER_ID", "")
    if ":rust:" not in wrapper_id or os.environ.get("XMTP_DEV_SHELL") != "rust":
        raise ValueError("identity command did not enter the production Rust shell")
    tools = {}
    for name, arguments in (
        ("rustc", ["-vV"]),
        ("cargo", ["--version"]),
        ("node", ["--version"]),
        ("python3.11", ["--version"]),
    ):
        executable = shutil.which(name)
        if not executable or not str(Path(executable).resolve()).startswith(
            "/nix/store/"
        ):
            raise ValueError(f"Rust shell tool is not pinned by Nix: {name}")
        tools[name] = {
            "path": executable,
            "resolvedPath": str(Path(executable).resolve()),
            "version": subprocess.check_output([executable, *arguments], text=True),
        }
    write(
        directory / "rust-identity.json", {**absence, "shell": "rust", "tools": tools}
    )
    print("SDK_RUST_SHELL_PROOF_IDENTITY_OK")


def prepare(directory, target):
    directory = outside_checkout(directory)
    absence = no_xcode()
    if os.environ.get("GITHUB_ACTIONS") != "true":
        raise ValueError("prepare is restricted to the hosted proof")
    if os.environ.get("BASH_ENV"):
        raise ValueError("proof requires no existing BASH_ENV")
    sha = subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True
    ).strip()
    if sha != os.environ["GITHUB_SHA"]:
        raise ValueError("proof checkout differs from CI head")
    if directory.exists() and any(directory.iterdir()):
        raise ValueError("proof directory is not empty")
    real_nix = shutil.which("nix")
    if not real_nix:
        raise ValueError("Nix must be installed before proof setup")
    binary = create_guard(directory, real_nix)
    write(
        directory / "state.json",
        {
            "target": target,
            "repository": str(ROOT.resolve()),
            "checkoutSha": sha,
            "runId": int(os.environ["GITHUB_RUN_ID"]),
            "runAttempt": int(os.environ["GITHUB_RUN_ATTEMPT"]),
            "host": absence,
        },
    )
    environment = {
        **os.environ,
        "PATH": str(binary) + os.pathsep + os.environ["PATH"],
        "BASH_ENV": str(directory / "bash-env"),
    }
    # Probe the actual wrapper. These commands must stop before real Nix runs.
    for shell in ("default", "local"):
        result = subprocess.run(
            [str(binary / "nix"), "develop", f"{ROOT}#{shell}", "--command", "true"],
            env={**environment, "SDK_RUST_PROOF_PHASE": "probe"},
            capture_output=True,
            text=True,
        )
        if (
            result.returncode != 2
            or f"SDK_RUST_SHELL_PROOF_BLOCKED shell={shell}" not in result.stderr
        ):
            raise ValueError(f"proof guard failed its {shell} probe")
    subprocess.run(
        [
            str(ROOT / "dev/nix-shell"),
            "--shell",
            "rust",
            "--command",
            "python3.11",
            str(Path(__file__).resolve()),
            "identity",
            "--directory",
            str(directory),
        ],
        cwd=ROOT,
        env={**environment, "SDK_RUST_PROOF_PHASE": "identity"},
        check=True,
    )
    with Path(os.environ["GITHUB_PATH"]).open("a") as output:
        output.write(str(binary) + "\n")
    with Path(os.environ["GITHUB_ENV"]).open("a") as output:
        output.write("BASH_ENV=" + str(directory / "bash-env") + "\n")
    print("SDK_RUST_SHELL_PROOF_GUARD_READY")


def clear_raw(root):
    cache = root / "target/sdk-artifacts"
    if cache.resolve() != root.resolve() / "target/sdk-artifacts" or cache.is_symlink():
        raise ValueError("raw SDK cache must be an owned directory")
    existed = cache.exists()
    if existed:
        shutil.rmtree(cache)
    return {
        "path": str(cache),
        "existed": existed,
        "absentAfterClear": not cache.exists(),
    }


def cold(directory):
    state = read(directory / "state.json")
    if Path(state["repository"]) != ROOT.resolve():
        raise ValueError("proof repository differs")
    write(directory / "cold.json", clear_raw(ROOT))
    print("SDK_RUST_SHELL_PROOF_RAW_CACHE_CLEARED")


def check_roles(index, target):
    if set(index["artifacts"]) != ROLES[target]:
        raise ValueError("proof raw role set differs")
    execution = index["execution"]
    if (
        len(execution) != len(ROLES[target])
        or {item["role"] for item in execution} != ROLES[target]
        or any(item["action"] != "build" for item in execution)
    ):
        raise ValueError("proof requires every raw SDK role to BUILD")


def verify(directory, target):
    state = read(directory / "state.json")
    if (
        state["target"] != target
        or not read(directory / "cold.json")["absentAfterClear"]
    ):
        raise ValueError("proof cold input differs")
    index = read(ROOT / "target/sdk-artifacts/artifacts.json")
    check_roles(index, target)
    manifest = read(ROOT / f"target/ci-products/sdk-{target}.manifest.json")
    for field in ("checkoutSha", "runId", "runAttempt"):
        if manifest[field] != state[field]:
            raise ValueError(f"proof product identity differs: {field}")
    context = manifest["context"]
    if (
        context["os"] != "linux"
        or context["target"] != target
        or context["profile"] != "debug"
        or context["features"]
        or context["instrumentation"] != "none"
    ):
        raise ValueError("proof product context differs")
    compiler = read(directory / "rust-identity.json")["tools"]["rustc"]["version"]
    if context["compilerIdentity"] != compiler:
        raise ValueError("proof product compiler differs from Rust shell")
    for tree in TREES[target]:
        if f"generated/{tree}/sdk-contract.json" not in manifest["files"]:
            raise ValueError(f"proof generated root missing: {tree}")
    if target == "browser" and not any(
        "worker" in name and name.endswith(".js") for name in manifest["files"]
    ):
        raise ValueError("proof Browser worker asset missing")
    calls = [
        json.loads(line)
        for line in (directory / "nix-calls.jsonl").read_text().splitlines()
    ]
    probes = [item for item in calls if not item["allowed"]]
    if (
        len(probes) != 2
        or {item["shell"] for item in probes} != {"default", "local"}
        or any(item["phase"] != "probe" for item in probes)
    ):
        raise ValueError("proof found a blocked production shell entry")
    if not any(
        item["command"] == "develop"
        and item["shell"] == "rust"
        and item["phase"] == "run"
        for item in calls
    ):
        raise ValueError("proof SDK build did not enter Rust shell")
    write(
        directory / "result.json",
        {
            "success": True,
            "target": target,
            "checkoutSha": state["checkoutSha"],
            "runId": state["runId"],
            "runAttempt": state["runAttempt"],
            "sourceHash": manifest["sourceHash"],
            "generatorHash": manifest["generatorHash"],
            "context": context,
            "execution": index["execution"],
            "generatedRoots": sorted(TREES[target]),
            "fileCount": len(manifest["files"]),
            "nixCalls": calls,
            "scope": "Node/Browser builds and Swift/Kotlin rendering; no iOS platform compilation",
        },
    )
    print("SDK_RUST_SHELL_PROOF_COLD_PRODUCTS_OK")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    for command in ("prepare", "identity", "cold", "verify", "guard"):
        subparser = subparsers.add_parser(command)
        subparser.add_argument("--directory", type=Path, required=True)
        if command in ("prepare", "verify"):
            subparser.add_argument("--target", choices=ROLES, required=True)
        if command == "guard":
            subparser.add_argument("arguments", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    if args.command == "guard":
        return guard(args.directory, args.arguments)
    if args.command in ("prepare", "verify"):
        globals()[args.command](args.directory, args.target)
    else:
        globals()[args.command](args.directory)
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        print(f"SDK_RUST_SHELL_PROOF_FAILED: {error}", file=sys.stderr)
        sys.exit(1)
