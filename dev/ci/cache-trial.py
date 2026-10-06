#!/usr/bin/env python3
"""Measure SDK generation with fresh Cargo targets and a separate cache store."""

import argparse
from collections import Counter
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[2]
ARMS = ("current", "no-incremental", "sccache", "kache")
HIDDEN_ENV = ("CI", "XMTP_TEST_LOGGING")
HITS = {"local_hit", "remote_hit", "prefetch_hit"}


def save(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")
    temporary.replace(path)


def source_identity(root):
    names = (
        subprocess.check_output(["git", "ls-files", "-z"], cwd=root)
        .decode()
        .split("\0")
    )
    files = {
        name: hashlib.sha256((root / name).read_bytes()).hexdigest()
        for name in names
        if name and (root / name).is_file()
    }
    return {
        "sha": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root)
        .decode()
        .strip(),
        "filesHash": hashlib.sha256(
            json.dumps(files, sort_keys=True).encode()
        ).hexdigest(),
    }


def events_summary(events, verify=False):
    builds = [e for e in events if "result" in e and "crate_name" in e]
    if not builds:
        raise ValueError("No compiler cache events; cache wiring is not proved")
    comparisons = [e.get("verify_compare", "") for e in builds if e["result"] in HITS]
    faults = [
        v for v in comparisons if v and v != "ok" and not v.startswith("path-debug:")
    ]
    if faults:
        raise ValueError("Kache content mismatch: " + "; ".join(faults))
    if verify and (not comparisons or any(not v for v in comparisons)):
        raise ValueError(
            "Verification requires a comparison for every hit and at least one hit"
        )
    units = {}
    for event in builds:
        name = event["crate_name"]
        row = units.setdefault(
            name,
            {
                "results": Counter(),
                "compileMs": 0,
                "wrapperMs": 0,
                "comparisons": [],
                "passthroughReasons": [],
            },
        )
        row["results"][event["result"]] += 1
        row["compileMs"] += event.get("compile_time_ms", 0)
        row["wrapperMs"] += event.get("elapsed_ms", 0)
        if event.get("verify_compare"):
            row["comparisons"].append(event["verify_compare"])
        if event.get("passthrough_reason"):
            row["passthroughReasons"].append(event["passthrough_reason"])
    return {
        "results": Counter(e["result"] for e in builds),
        "units": units,
        "verification": "compared" if verify else "not requested",
    }


def environment(arm, store, runtime, inherited, verify=False):
    env = dict(inherited)
    for key in (
        "RUSTC_WRAPPER",
        "RUSTC_WORKSPACE_WRAPPER",
        "CARGO_BUILD_RUSTC_WRAPPER",
        "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER",
        "CARGO_INCREMENTAL",
    ):
        env.pop(key, None)
    if arm != "current":
        env["CARGO_INCREMENTAL"] = "0"
    if arm == "kache":
        env.update(
            RUSTC_WRAPPER=inherited.get("RUSTC_WRAPPER") or "kache",
            KACHE_CACHE_DIR=str(store),
            KACHE_RUNTIME_DIR=str(runtime),
            KACHE_CONFIG=str(runtime / "config.toml"),
            KACHE_HOST_CONFIG=str(runtime / "host.toml"),
            KACHE_ADAPTIVE_INCREMENTAL="0",
            KACHE_PRESERVE_INCREMENTAL="0",
            KACHE_CACHE_EXECUTABLES="1",
            KACHE_VERIFY="1" if verify else "0",
            KACHE_KEY_ENV_VARS=",".join(HIDDEN_ENV),
            KACHE_REMOTE_READONLY="1",
            KACHE_BUILD_SCRIPT_CACHE="0",
            KACHE_LOG="kache=info",
        )
    elif arm == "sccache":
        env.update(
            RUSTC_WRAPPER="sccache", SCCACHE_DIR=str(store), SCCACHE_GHA_ENABLED="false"
        )
        # Each sample owns its server. Do not stop a user's shared server.
        with socket.socket() as sock:
            sock.bind(("127.0.0.1", 0))
            env["SCCACHE_SERVER_PORT"] = str(sock.getsockname()[1])
        for key in list(env):
            if key.startswith(
                (
                    "SCCACHE_BUCKET",
                    "SCCACHE_ENDPOINT",
                    "SCCACHE_REGION",
                    "SCCACHE_REDIS",
                    "SCCACHE_MEMCACHED",
                    "SCCACHE_S3",
                    "SCCACHE_AZURE",
                    "SCCACHE_GCS",
                    "SCCACHE_WEBDAV",
                )
            ):
                env.pop(key)
    return env


def run(command, cwd, env, log):
    with log.open("ab") as stream:
        stream.write(("COMMAND " + json.dumps(command) + "\n").encode())
        stream.flush()
        process = subprocess.run(
            command, cwd=cwd, env=env, stdout=stream, stderr=subprocess.STDOUT
        )
    if process.returncode:
        raise RuntimeError(f"Command failed ({process.returncode}); see {log}")


def init(args):
    if args.output.exists():
        raise ValueError("Trial output already exists; choose a new isolated directory")
    args.output.mkdir(parents=True)
    state = {
        "schemaVersion": 1,
        "arm": args.arm,
        "source": source_identity(ROOT),
        "question": "Can the wrapper cache native SDK and bindgen with fresh Cargo targets?",
        "stopCondition": "One cold/warm pair, cache passthrough, or a build/setup failure",
        "samples": [],
        "qualification": "UNVERIFIED: this feasibility run does not adopt a cache",
        "hiddenEnv": {name: os.environ.get(name) for name in HIDDEN_ENV},
        "inputCasesRemaining": [
            "ordinary Rust edit",
            "generator edit",
            "feature change",
            "toolchain change",
            "embedded file change",
            "hidden environment change",
        ],
        "job": {
            "runId": os.environ.get("GITHUB_RUN_ID"),
            "attempt": os.environ.get("GITHUB_RUN_ATTEMPT"),
            "refProtected": os.environ.get("GITHUB_REF_PROTECTED"),
            "vcpus": 16,
            "completeAllocatedCost": "UNVERIFIED: use final job API timestamps; include post steps and cancelled jobs",
        },
    }
    save(args.output / "trial.json", state)


def sample(args):
    if os.environ.get("XMTP_NIX_ENV") != "yes":
        raise ValueError("Run the trial inside dev/nix-shell")
    if sys.platform != "linux":
        raise ValueError("The SDK timing trial requires the Linux runner")
    state = json.loads((args.output / "trial.json").read_text())
    arm = state["arm"]
    if args.phase == "verify" and arm != "kache":
        raise ValueError("Verification is a separate Kache qualification sample")
    path = args.output / f"{args.phase}-{args.index}"
    path.mkdir()  # Reusing a sample would hide Cargo freshness.
    started = time.monotonic()
    row = {
        "phase": args.phase,
        "index": args.index,
        "status": "running",
        "startedAt": datetime.now(timezone.utc).isoformat(),
        "timedArm": args.phase != "verify",
        "secondCheckout": args.second_checkout,
    }
    state["samples"].append(row)
    save(args.output / "trial.json", state)
    runtime = path / "runtime"
    runtime.mkdir(mode=0o700)
    (runtime / "host.toml").write_text("")
    (runtime / "config.toml").write_text(
        '[cache]\nkey_env_vars = ["CI", "XMTP_TEST_LOGGING"]\nadaptive_incremental = false\npreserve_incremental = false\ncache_executables = true\n'
    )
    log = path / "build.log"
    env = dict(os.environ)
    store = args.output / f"store-{args.index}"
    cwd = ROOT
    cache_wrapper = None
    try:
        if args.phase != "cold":
            cold = [
                s
                for s in state["samples"]
                if s["phase"] == "cold" and s["status"] == "passed"
            ]
            if not cold:
                raise ValueError("A warm sample requires a completed cold sample")
            store = args.output / f"store-{cold[-1]['index']}"
        elif store.exists():
            raise ValueError("Cold cache store already exists")
        env = environment(arm, store, runtime, env, args.phase == "verify")
        if args.second_checkout:
            cwd = path / "checkout"
            run(
                ["git", "clone", "--local", "--no-hardlinks", str(ROOT), str(cwd)],
                ROOT,
                env,
                log,
            )
            run(["git", "checkout", "--detach", state["source"]["sha"]], cwd, env, log)
            if (ROOT / "node_modules").is_dir():
                (cwd / "node_modules").symlink_to(
                    ROOT / "node_modules", target_is_directory=True
                )
        if source_identity(cwd) != state["source"]:
            raise ValueError("Trial source changed after init")
        row["effectiveEnvironment"] = {
            key: env.get(key)
            for key in (
                "CARGO_INCREMENTAL",
                "RUSTC_WRAPPER",
                "RUSTC_WORKSPACE_WRAPPER",
                "KACHE_ADAPTIVE_INCREMENTAL",
                "KACHE_PRESERVE_INCREMENTAL",
                "KACHE_KEY_ENV_VARS",
                "KACHE_VERIFY",
                "KACHE_CACHE_DIR",
                "KACHE_RUNTIME_DIR",
                "CI",
                "XMTP_TEST_LOGGING",
                "RUSTFLAGS",
                "CARGO_ENCODED_RUSTFLAGS",
                "OPENSSL_NO_VENDOR",
                "OPENSSL_STATIC",
                "NIX_DEVSHELL",
            )
        }
        row["compiler"] = subprocess.check_output(
            ["rustc", "-vV"], cwd=cwd, env=env
        ).decode()
        row["cargo"] = (
            subprocess.check_output(["cargo", "--version"], cwd=cwd, env=env)
            .decode()
            .strip()
        )
        context = {
            "compiler": row["compiler"],
            "flags": {
                name: value
                for name, value in env.items()
                if name
                in (
                    "CI",
                    "XMTP_TEST_LOGGING",
                    "RUSTFLAGS",
                    "CARGO_ENCODED_RUSTFLAGS",
                    "CC",
                    "CXX",
                    "AR",
                    "CFLAGS",
                    "CXXFLAGS",
                    "LDFLAGS",
                    "SDKROOT",
                    "OPENSSL_NO_VENDOR",
                    "OPENSSL_STATIC",
                    "OPENSSL_DIR",
                    "OPENSSL_LIB_DIR",
                )
                or name.startswith(
                    ("CARGO_TARGET_", "CARGO_PROFILE_", "CC_", "CXX_", "AR_", "CFLAGS_")
                )
            },
            "profile": "debug",
            "features": "",
        }
        if state.get("context") not in (None, context):
            raise ValueError("Trial compiler context changed after the first sample")
        state["context"] = context
        if arm in ("sccache", "kache"):
            wrapper = shutil.which(env["RUSTC_WRAPPER"], path=env.get("PATH"))
            if not wrapper or Path(wrapper).name != arm:
                raise ValueError(
                    "Effective compiler wrapper does not match the trial arm"
                )
            env["RUSTC_WRAPPER"] = wrapper
            cache_wrapper = wrapper
            run(["dev/agent-run", wrapper, "--version"], cwd, env, log)
        if arm == "sccache":
            run(["sccache", "--start-server"], cwd, env, log)
            run(["sccache", "--zero-stats"], cwd, env, log)
        command = [
            "bash",
            "crates/xmtp_sdk/dev/generate",
            "--targets",
            "node",
            "--artifacts",
            str(path / "artifacts"),
            "--out",
            str(path / "generated"),
        ]
        row["command"] = command
        save(args.output / "trial.json", state)
        build_started = time.monotonic()
        run(command, cwd, env, log)
        row["generationSeconds"] = time.monotonic() - build_started
        row["products"] = {
            str(p.relative_to(path / "generated")): hashlib.sha256(
                p.read_bytes()
            ).hexdigest()
            for p in (path / "generated").rglob("*")
            if p.is_file()
        }
        if not row["products"]:
            raise ValueError("The real SDK generator made no product")
        if arm == "kache":
            events_file = runtime / "events.jsonl"
            events = [
                json.loads(line)
                for line in events_file.read_text().splitlines()
                if line.strip()
            ]
            row["cache"] = events_summary(events, args.phase == "verify")
            report = subprocess.run(
                [env["RUSTC_WRAPPER"], "stats", "--full", "--json"],
                env=env,
                cwd=cwd,
                capture_output=True,
                check=True,
            )
            (path / "cache-stats.json").write_bytes(report.stdout)
        elif arm == "sccache":
            report = subprocess.run(
                ["sccache", "--show-stats", "--stats-format", "json"],
                env=env,
                cwd=cwd,
                capture_output=True,
                check=True,
            )
            (path / "cache-stats.json").write_bytes(report.stdout)
            row["cache"] = json.loads(report.stdout)
            row["unitResults"] = (
                "UNVERIFIED: sccache aggregate stats do not prove per-unit hits"
            )
        else:
            row["cache"] = {"wrapper": "disabled"}
        row["status"] = "passed"
    except BaseException as error:
        row["status"] = "failed"
        row["error"] = str(error)
        raise
    finally:
        if arm == "sccache" and cache_wrapper:
            subprocess.run(
                ["sccache", "--stop-server"],
                cwd=ROOT,
                env=env,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                check=False,
            )
        if arm == "kache" and cache_wrapper:
            subprocess.run(
                [env["RUSTC_WRAPPER"], "daemon", "stop"],
                cwd=ROOT,
                env=env,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                check=False,
            )
        row["sampleSecondsThroughTeardown"] = time.monotonic() - started
        row["sampleCoreMinutes"] = row["sampleSecondsThroughTeardown"] * 16 / 60
        save(args.output / "trial.json", state)
        print(json.dumps(row, indent=2, sort_keys=True))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="action", required=True)
    setup = sub.add_parser("init")
    setup.add_argument("--arm", choices=ARMS, required=True)
    setup.add_argument("--output", type=Path, required=True)
    build = sub.add_parser("sample")
    build.add_argument("--output", type=Path, required=True)
    build.add_argument("--phase", choices=("cold", "warm", "verify"), required=True)
    build.add_argument("--index", type=int, default=0)
    build.add_argument("--second-checkout", action="store_true")
    args = parser.parse_args()
    args.output = args.output.resolve()
    if args.action == "init":
        init(args)
    else:
        sample(args)


if __name__ == "__main__":
    main()
