#!/usr/bin/env python3
"""Run a reviewed security-hardening manifest inside the sbx VM."""

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import time
import uuid


LANE_ID = re.compile(r"[a-z0-9][a-z0-9-]*\Z")
EFFORT = {"low", "medium", "high", "xhigh", "max", "ultra"}
ACCESS = {"standard", "daybreak_blue", "daybreak_red"}
PHASES = {"discovery", "validation", "review"}


def now():
    return datetime.now(timezone.utc).isoformat()


def write_json(path, value):
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(value, indent=2) + "\n")
    temporary.replace(path)


def inside(run, relative):
    if not isinstance(relative, str) or not relative or Path(relative).is_absolute():
        raise ValueError("manifest file paths must be relative to the run directory")
    path = (run / relative).resolve()
    if not path.is_relative_to(run):
        raise ValueError("manifest file path escapes the run directory")
    return path


def manifest(run):
    run = run.resolve()
    data = json.loads((run / "manifest.json").read_text())
    repo = Path(data["repo"]).expanduser().resolve()
    if not repo.is_dir() or not (repo / ".git").exists():
        raise ValueError("manifest repo must be a Git clone")
    if run.is_relative_to(repo):
        raise ValueError("run directory must be outside the repository")
    base = data["base"]
    if not isinstance(base, str) or not re.fullmatch(r"[0-9a-f]{40}", base):
        raise ValueError("manifest base must be a full commit SHA")
    concurrency = data.get("concurrency", 4)
    if type(concurrency) is not int or not 1 <= concurrency <= 32:
        raise ValueError("manifest concurrency must be 1..32")
    if data.get("phase", "discovery") not in PHASES:
        raise ValueError("manifest phase must be discovery, validation, or review")
    lanes = data["lanes"]
    if not isinstance(lanes, list) or not lanes:
        raise ValueError("manifest needs at least one lane")
    seen = set()
    for lane in lanes:
        lane_id = lane["id"]
        if not isinstance(lane_id, str) or not LANE_ID.fullmatch(lane_id) or lane_id in seen:
            raise ValueError("lane IDs must be unique lowercase names")
        seen.add(lane_id)
        if lane["cli"] not in {"claude", "codex"}:
            raise ValueError(f"unknown CLI for {lane_id}")
        if not isinstance(lane["model"], str) or not lane["model"]:
            raise ValueError(f"missing model for {lane_id}")
        if lane["effort"] not in EFFORT or (lane["cli"] == "claude" and lane["effort"] == "ultra"):
            raise ValueError(f"unsupported effort for {lane_id}")
        if not inside(run, lane["prompt"]).is_file():
            raise ValueError(f"missing prompt for {lane_id}")
        if "settings" in lane and not inside(run, lane["settings"]).is_file():
            raise ValueError(f"missing settings for {lane_id}")
        if "cyber_access" in lane and (
            lane["cli"] != "codex" or lane["cyber_access"] not in ACCESS
        ):
            raise ValueError(f"invalid cyber access for {lane_id}")
        if "max_budget_usd" in lane and (
            lane["cli"] != "claude"
            or type(lane["max_budget_usd"]) not in {int, float}
            or lane["max_budget_usd"] <= 0
        ):
            raise ValueError(f"invalid Claude budget for {lane_id}")
    return data, repo


def git(repo, *args):
    return subprocess.run(
        ["git", "-C", str(repo), *args], text=True, capture_output=True, check=True
    ).stdout.strip()


def slot_names(data):
    if data.get("phase", "discovery") == "validation":
        return [f"slot-{number:02d}" for number in range(1, min(data.get("concurrency", 4), len(data["lanes"])) + 1)]
    return ["shared"]


def init(run):
    data, repo = manifest(run)
    actual = git(repo, "rev-parse", "--verify", f"{data['base']}^{{commit}}")
    if actual != data["base"]:
        raise ValueError("base SHA did not resolve to the requested commit")
    worktrees = run / "worktrees"
    worktrees.mkdir(exist_ok=True)
    for name in slot_names(data):
        destination = worktrees / name
        if destination.exists():
            if git(destination, "rev-parse", "HEAD") != actual:
                raise ValueError(f"existing worktree has a different commit: {name}")
        else:
            subprocess.run(
                ["git", "-C", str(repo), "worktree", "add", "--detach", str(destination), actual],
                check=True,
            )
    for lane in data["lanes"]:
        lane_id = lane["id"]
        (run / "lanes" / lane_id).mkdir(parents=True, exist_ok=True)
    print(f"Prepared {len(slot_names(data))} worktrees at {actual}")


def process_alive(pid):
    try:
        os.kill(pid, 0)
        proc_stat = Path(f"/proc/{pid}/stat")
        if proc_stat.exists() and proc_stat.read_text().split()[2] == "Z":
            return False
        return True
    except (ProcessLookupError, FileNotFoundError):
        return False


def lane_state(run, lane_id):
    directory = run / "lanes" / lane_id
    result = directory / "result.json"
    if result.exists():
        return json.loads(result.read_text())["state"]
    pid_file = directory / "pid"
    if pid_file.exists():
        return "running" if process_alive(int(pid_file.read_text())) else "died"
    return "pending"


def status(run):
    data, _ = manifest(run)
    rows = {lane["id"]: lane_state(run, lane["id"]) for lane in data["lanes"]}
    print(json.dumps(rows, indent=2))
    return rows


def launch(run, lane, slot):
    lane_id = lane["id"]
    directory = run / "lanes" / lane_id
    (directory / "slot").write_text(f"{slot}\n")
    with (directory / "supervisor.log").open("ab") as log:
        process = subprocess.Popen(
            [sys.executable, str(Path(__file__).resolve()), "_run", str(run), lane_id],
            stdin=subprocess.DEVNULL,
            stdout=log,
            stderr=subprocess.STDOUT,
            start_new_session=True,
            close_fds=True,
        )
    (directory / "pid").write_text(f"{process.pid}\n")


def supervise(run):
    data, _ = manifest(run)
    limit = data.get("concurrency", 4)
    validation = data.get("phase", "discovery") == "validation"
    while True:
        try:
            while os.waitpid(-1, os.WNOHANG)[0] != 0:
                pass
        except ChildProcessError:
            pass
        states = {lane["id"]: lane_state(run, lane["id"]) for lane in data["lanes"]}
        pending = [lane for lane in data["lanes"] if states[lane["id"]] == "pending"]
        running = sum(value == "running" for value in states.values())
        if not pending and not running:
            return
        if validation:
            occupied = {
                (run / "lanes" / lane["id"] / "slot").read_text().strip()
                for lane in data["lanes"]
                if states[lane["id"]] in {"running", "died"}
                and (run / "lanes" / lane["id"] / "slot").exists()
            }
            available = [name for name in slot_names(data) if name not in occupied]
        else:
            available = ["shared"] * max(0, limit - running)
        launched = 0
        for lane, slot in zip(pending, available[: max(0, limit - running)]):
            launch(run, lane, slot)
            launched += 1
        if pending and not running and not launched:
            return  # A died lane owns every remaining validation slot.
        time.sleep(1)


def start(run):
    data, _ = manifest(run)
    if any(not (run / "worktrees" / name).is_dir() for name in slot_names(data)):
        raise ValueError("run init before start")
    pid_file = run / "supervisor.pid"
    if pid_file.exists() and process_alive(int(pid_file.read_text())):
        print(f"Supervisor already running: {pid_file.read_text().strip()}")
        return
    with (run / "supervisor.log").open("ab") as log:
        process = subprocess.Popen(
            [sys.executable, str(Path(__file__).resolve()), "_supervise", str(run)],
            stdin=subprocess.DEVNULL,
            stdout=log,
            stderr=subprocess.STDOUT,
            start_new_session=True,
            close_fds=True,
        )
    pid_file.write_text(f"{process.pid}\n")
    print(f"Supervisor started: {process.pid}")


def snapshot_and_reset(worktree, directory, run, lane_id, base):
    """Save all tracked and untracked validation edits before reusing a slot."""
    index = directory / "snapshot.index"
    environment = os.environ.copy()
    environment.update(
        GIT_INDEX_FILE=str(index),
        GIT_AUTHOR_NAME="Security hardening audit",
        GIT_AUTHOR_EMAIL="audit@localhost",
        GIT_COMMITTER_NAME="Security hardening audit",
        GIT_COMMITTER_EMAIL="audit@localhost",
    )

    def capture(*args, env=environment):
        return subprocess.run(
            ["git", "-C", str(worktree), *args],
            text=True, capture_output=True, check=True, env=env,
        ).stdout.strip()

    capture("read-tree", base)
    capture("add", "-A")
    tree = capture("write-tree")
    commit = capture("commit-tree", tree, "-p", base, "-m", f"Audit validation {lane_id}")
    run_key = hashlib.sha256(str(run).encode()).hexdigest()[:12]
    ref = f"refs/security-hardening/{run_key}/{lane_id}"
    capture("update-ref", ref, commit)
    # The ref now keeps even untracked repro files. Restore the slot for its next lane.
    capture("reset", "--hard", base, env=os.environ.copy())
    capture("clean", "-fd", env=os.environ.copy())
    index.unlink(missing_ok=True)
    return ref


def run_lane(run, lane_id):
    data, repo = manifest(run)
    lane = next(item for item in data["lanes"] if item["id"] == lane_id)
    directory = run / "lanes" / lane_id
    slot = (directory / "slot").read_text().strip()
    worktree = run / "worktrees" / slot
    prompt = inside(run, lane["prompt"])
    session_id = str(uuid.uuid4()) if lane["cli"] == "claude" else None
    record = {
        "state": "running",
        "started_at": now(),
        "session_id": session_id,
        "prompt": str(prompt),
        "prompt_sha256": hashlib.sha256(prompt.read_bytes()).hexdigest(),
        "worktree": str(worktree),
        "phase": data.get("phase", "discovery"),
        "cli": lane["cli"],
        "provider": lane.get("provider"),
        "model": lane["model"],
        "effort": lane["effort"],
        "cyber_access": lane.get("cyber_access"),
    }
    write_json(directory / "session.json", record)
    if lane["cli"] == "claude":
        command = [
            "claude", "-p", "--model", lane["model"], "--effort", lane["effort"],
            "--session-id", session_id, "--output-format", "stream-json", "--verbose",
            "--permission-mode", "bypassPermissions",
        ]
        if "settings" in lane:
            command += ["--settings", str(inside(run, lane["settings"]))]
        if "max_budget_usd" in lane:
            command += ["--max-budget-usd", str(lane["max_budget_usd"])]
        command += ["--add-dir", str(directory)]
    else:
        command = [
            "codex", "exec", "--json", "-C", str(worktree), "-m", lane["model"],
            "-s", "workspace-write", "-c", 'approval_policy="never"',
            "-c", f'model_reasoning_effort="{lane["effort"]}"',
            "-c", "sandbox_workspace_write.network_access=true",
            "--add-dir", str(directory), "--add-dir", str(repo / ".git"),
            "-o", str(directory / "last.md"),
        ]
        if "cyber_access" in lane:
            command += ["--cyber-access-program", lane["cyber_access"]]
    events = directory / "events.jsonl"
    errors = directory / "stderr.log"
    environment = os.environ.copy()
    if lane["cli"] == "claude":
        environment.update(BASH_DEFAULT_TIMEOUT_MS="3600000", BASH_MAX_TIMEOUT_MS="3600000")
    with prompt.open("rb") as input_file, events.open("wb") as output_file, errors.open("wb") as error_file:
        completed = subprocess.run(
            command, cwd=worktree, stdin=input_file, stdout=output_file, stderr=error_file,
            env=environment, check=False,
        )
    final = None
    for line in events.read_text(errors="replace").splitlines():
        try:
            event = json.loads(line)
        except json.JSONDecodeError:
            continue
        if lane["cli"] == "claude" and event.get("type") == "result":
            final = event
        if lane["cli"] == "codex" and event.get("type") in {"turn.completed", "turn.failed"}:
            final = event
    if lane["cli"] == "codex":
        for line in events.read_text(errors="replace").splitlines():
            try:
                event = json.loads(line)
            except json.JSONDecodeError:
                continue
            if event.get("type") == "thread.started":
                record["session_id"] = event.get("thread_id")
                break
    good = completed.returncode == 0 and final is not None
    if lane["cli"] == "claude":
        good = good and not final.get("is_error") and not final.get("permission_denials")
    else:
        good = good and final.get("type") == "turn.completed"
    record.update(
        state="done" if good else "failed",
        ended_at=now(),
        exit_code=completed.returncode,
        final_event_type=final.get("type") if final else None,
    )
    if data.get("phase", "discovery") == "validation":
        record["snapshot_ref"] = snapshot_and_reset(worktree, directory, run, lane_id, data["base"])
    write_json(directory / "result.json", record)


def stop(run, lane_id):
    directory = run / "lanes" / lane_id
    if not directory.is_dir():
        raise ValueError(f"unknown lane: {lane_id}")
    if (directory / "result.json").exists():
        print(f"Already finished: {lane_id}")
        return
    pid_file = directory / "pid"
    if not pid_file.exists():
        raise ValueError(f"lane has not started: {lane_id}")
    pid = int(pid_file.read_text())
    if process_alive(pid):
        os.killpg(pid, signal.SIGTERM)
    write_json(directory / "result.json", {"state": "stopped", "ended_at": now()})
    print(f"Stopped {lane_id}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("init", "start", "status", "stop", "_supervise", "_run"))
    parser.add_argument("run", type=Path)
    parser.add_argument("lane", nargs="?")
    args = parser.parse_args()
    run = args.run.expanduser().resolve()
    try:
        if args.action == "init":
            init(run)
        elif args.action == "start":
            start(run)
        elif args.action == "status":
            status(run)
        elif args.action == "stop":
            if not args.lane:
                raise ValueError("stop requires a lane ID")
            stop(run, args.lane)
        elif args.action == "_supervise":
            supervise(run)
        elif args.action == "_run":
            if not args.lane:
                raise ValueError("_run requires a lane ID")
            run_lane(run, args.lane)
    except (OSError, ValueError, KeyError, subprocess.CalledProcessError) as error:
        print(f"guest_lanes: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
