"""Shared driver helpers for cross-version-test and cross-talk-test.

Mechanism, not policy: this module exposes subprocess + git +
plan-parsing primitives; the driver scripts decide rendering and
exit codes.
"""

from __future__ import annotations

import argparse
import contextlib
import json
import os
import re
import subprocess
import sys
import tempfile
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Any, TypedDict

from git import BadName, Repo
from packaging.version import InvalidVersion, Version
from rich.console import Console
from rich.table import Table


def _console_width() -> int | None:
    """Force a wide table in CI (GH Actions reports 80 cols but doesn't
    actually wrap), keep auto-sizing on real TTYs."""
    if os.environ.get("CI") or os.environ.get("GITHUB_ACTIONS"):
        return 200
    return None


_stderr_console = Console(stderr=True, highlight=False, width=_console_width())

NIGHTLY_TAG_RE = re.compile(
    r"^(?:node-bindings|wasm-bindings|android|ios)-.*-nightly\.\d{8}\.[0-9a-f]{7}$"
)
NIGHTLY_PARSE_RE = re.compile(r".*-nightly\.(\d{8})\.([0-9a-f]{7})$")

STATUS_PLAIN = {
    "PASS": "✓ PASS",
    "FAIL": "✗ FAIL",
    "SKIP-BUILD": "⊘ SKIP (build failed)",
    "SKIP-NOT-RUN": "⊘ SKIP (not run — earlier required failure)",
    "SKIP-NO-HEALTHCHECK": "⊘ SKIP (no healthcheck subcommand)",
}
STATUS_MD = {
    "PASS": "✅ PASS",
    "FAIL": "❌ FAIL",
    "SKIP-BUILD": "⊘ SKIP (build failed)",
    "SKIP-NOT-RUN": "⊘ SKIP (not run — earlier required failure)",
    "SKIP-NO-HEALTHCHECK": "⊘ SKIP (no healthcheck subcommand)",
}


def eputs(*parts: object) -> None:
    print(*parts, file=sys.stderr, flush=True)


def gh_error(msg: str) -> None:
    eputs(f"::error::{msg}")


def gh_warning(msg: str) -> None:
    eputs(f"::warning::{msg}")


def gh_notice(msg: str) -> None:
    eputs(f"::notice::{msg}")


@contextlib.contextmanager
def gh_group(label: str):
    """Emit a GitHub-Actions log group around the block. Closes even on
    exception so :: markers aren't dropped on early exit."""
    eputs(f"::group::{label}")
    try:
        yield
    finally:
        eputs("::endgroup::")


@contextlib.contextmanager
def with_spinner(label: str):
    """Stderr spinner suppressed in CI / non-TTY. Yields control to caller."""
    if (
        not sys.stderr.isatty()
        or os.environ.get("CI")
        or os.environ.get("GITHUB_ACTIONS")
    ):
        yield
        return
    with _stderr_console.status(label):
        yield


# ---------- Plan entries ----------


class ResultRow(TypedDict, total=False):
    status: str
    short: str
    sha: str
    kind: str
    branch: str
    label: str


@dataclass(frozen=True, slots=True)
class Entry:
    role: str = ""
    sha: str = ""
    short: str = ""
    branch: str = ""
    label: str = ""
    kind: str = ""

    @property
    def required(self) -> bool:
        return self.kind != "nightly"


def classify_kind(label: str, branch: str) -> str:
    if label.startswith("nightly."):
        return "nightly"
    if label.startswith("HEAD"):
        return "head"
    if not label:
        return "head" if not branch else "stable"
    gh_warning(f"unrecognized plan label '{label}'; treating as nightly")
    return "nightly"


def parse_plan_entry(raw: dict[str, Any]) -> Entry:
    sha = raw.get("sha") or ""
    short = raw.get("short") or (sha[:7] if sha else "")
    branch = raw.get("branch") or ""
    label = raw.get("label") or ""
    return Entry(
        role=raw.get("role") or "",
        sha=sha,
        short=short,
        branch=branch,
        label=label,
        kind=classify_kind(label, branch),
    )


def validate_entry(e: Entry) -> bool:
    """True if the entry is missing required sha/role."""
    return not e.role or e.role == "null" or not e.sha or e.sha == "null"


# ---------- Git ----------


_REPO: Repo | None = None
_REPO_ROOT: str | None = None


def _resolve_repo() -> tuple[Repo, str]:
    """One-time resolution of (Repo, working-tree root). Used by both
    `repo()` and `repo_root()` so we never run `jj root` twice."""
    try:
        plain = Repo(os.getcwd(), search_parent_directories=True)
        return plain, plain.working_tree_dir  # type: ignore[return-value]
    except Exception:
        pass
    r = subprocess.run(["jj", "root"], capture_output=True, text=True)
    if r.returncode != 0:
        raise RuntimeError("not in a git or jj working tree")
    jj_root = r.stdout.strip()
    # .jj/repo is a directory in the default workspace, or a file
    # containing the absolute path to the default workspace's repo dir.
    repo_dir_path = Path(jj_root) / ".jj" / "repo"
    repo_dir = (
        Path(repo_dir_path.read_text().strip())
        if repo_dir_path.is_file()
        else repo_dir_path
    )
    git_target = (repo_dir / "store" / "git_target").read_text().strip()
    git_dir = (repo_dir / "store" / git_target).resolve()
    return Repo(git_dir), jj_root


def repo() -> Repo:
    global _REPO, _REPO_ROOT
    if _REPO is None:
        _REPO, _REPO_ROOT = _resolve_repo()
    return _REPO


def repo_root() -> str:
    global _REPO, _REPO_ROOT
    if _REPO_ROOT is None:
        _REPO, _REPO_ROOT = _resolve_repo()
    return _REPO_ROOT


def git_for_each_ref(pattern: str) -> list[str]:
    """List ref shortnames matching a refs/remotes/<glob> pattern."""
    # GitPython doesn't expose for-each-ref directly with format; the underlying
    # git command stays the cleanest path for glob matching.
    out = repo().git.for_each_ref("--format=%(refname:short)", pattern)
    return [ln for ln in out.splitlines() if ln.strip()]


def git_tags_by_creator_date() -> list[str]:
    return [
        t.name
        for t in sorted(
            repo().tags, key=lambda t: t.commit.committed_datetime, reverse=True
        )
    ]


def git_rev_parse_multi(*refs: str) -> list[str]:
    return [repo().commit(r).hexsha for r in refs]


def git_rev_parse_verify(commitish: str) -> str | None:
    try:
        return repo().commit(commitish).hexsha
    except (BadName, ValueError):
        return None


def git_abbrev_ref_head() -> str | None:
    try:
        return repo().active_branch.name
    except TypeError:  # detached HEAD
        return None


def filter_nightly_tags(lines: list[str]) -> list[str]:
    out = []
    for ln in lines:
        name = re.sub(r".*\trefs/tags/", "", ln)
        name = re.sub(r"^refs/tags/", "", name).strip()
        if NIGHTLY_TAG_RE.match(name):
            out.append(name)
    return out


def bootstrap_fetch() -> int:
    """Fetch release branches + nightly tags. Returns nightly tag count."""
    origin = repo().remotes.origin
    with with_spinner("fetching release branches"):
        origin.fetch(
            "+refs/heads/release/*:refs/remotes/origin/release/*",
            no_tags=True,
            depth=1,
        )
    with with_spinner("listing remote tags"):
        ls = repo().git.ls_remote("--tags", "--refs", "origin")
    tags = filter_nightly_tags(ls.splitlines())
    if tags:
        with with_spinner(f"fetching {len(tags)} nightly tags"):
            origin.fetch(
                [f"refs/tags/{t}:refs/tags/{t}" for t in tags],
                no_tags=True,
                depth=1,
            )
    return len(tags)


# ---------- Version picking ----------


def parse_stable_branch(name: str) -> Version | None:
    try:
        return Version(name.removeprefix("v"))
    except InvalidVersion:
        return None


def last_two_stable_branches() -> list[str]:
    # Dedupe by (major, minor), keep highest patch within each pair.
    parsed: dict[tuple[int, int], tuple[Version, str]] = {}
    for ref in git_for_each_ref("refs/remotes/origin/release/*"):
        name = re.sub(r"^origin/release/", "", ref)
        ver = parse_stable_branch(name)
        if ver:
            parsed[(ver.major, ver.minor)] = (ver, name)
    sorted_branches = sorted(parsed.values(), key=lambda vn: vn[0])
    return [name for _, name in sorted_branches[-2:][::-1]]


def nightly_candidates() -> list[tuple[str, str]]:
    seen = set()
    out = []
    for t in git_tags_by_creator_date():
        m = NIGHTLY_PARSE_RE.match(t)
        if not m:
            continue
        key = (m.group(1), m.group(2))
        if key not in seen:
            seen.add(key)
            out.append(key)
    return out


def pick_versions(sample_size: int = 3) -> list[dict[str, Any]]:
    branches = last_two_stable_branches()
    if len(branches) < 2:
        raise RuntimeError(
            f"need at least 2 stable release branches; found {len(branches)}"
        )
    newest, next_b = branches
    newest_sha, next_sha, head_sha = git_rev_parse_multi(
        f"origin/release/{newest}", f"origin/release/{next_b}", "HEAD"
    )
    head_ref = git_abbrev_ref_head()
    head_label = f"HEAD@{head_ref}" if head_ref else "HEAD"

    # Sort key shape: (kind, comparable). 0=stable, 1=nightly, 2=head.
    # Mixing types is fine — tuples compare lexicographically, so the
    # leading int discriminator means we only compare like-with-like.
    def stable_key(name: str) -> tuple[int, Version]:
        return (0, parse_stable_branch(name) or Version("0"))

    base = [
        {
            "sha": newest_sha,
            "branch": f"release/{newest}",
            "label": "",
            "key": stable_key(newest),
        },
        {
            "sha": next_sha,
            "branch": f"release/{next_b}",
            "label": "",
            "key": stable_key(next_b),
        },
        {"sha": head_sha, "branch": "", "label": head_label, "key": (2,)},
    ]
    nightlies = []
    if sample_size > 0:
        for date, sha7 in nightly_candidates()[:sample_size]:
            full = git_rev_parse_verify(f"{sha7}^{{commit}}")
            if full:
                nightlies.append(
                    {
                        "sha": full,
                        "branch": "",
                        "label": f"nightly.{date}",
                        "key": (1, date),
                    }
                )

    seen, all_entries = set(), []
    for e in base + nightlies:
        if e["sha"] not in seen:
            seen.add(e["sha"])
            all_entries.append(e)
    all_entries.sort(key=lambda e: e["key"])

    creator_seen = False
    plan = []
    for e in all_entries:
        required = not e["label"]
        role = "creator" if (not creator_seen and required) else "sender"
        if role == "creator":
            creator_seen = True
        plan.append(
            {
                "sha": e["sha"],
                "short": e["sha"][:7],
                "role": role,
                "branch": e["branch"],
                "label": e["label"],
            }
        )
    return plan


# ---------- xdbg invocation ----------


def flake_ref(kind: str, sha: str) -> str:
    if kind == "head":
        root = os.environ.get("GITHUB_WORKSPACE") or repo_root()
        return f"path:{root}#xdbg"
    return f"github:xmtp/libxmtp/{sha}#xdbg"


def sandbox_env(env_extras: dict[str, str] | None = None) -> dict[str, str]:
    extras = env_extras or {}
    return {
        "XDBG_DB_ROOT": extras.get("XVT_DB_ROOT") or os.environ.get("XVT_DB_ROOT", "")
    }


def xdbg_invocation(kind: str, sha: str) -> list[str]:
    return ["nix", "run", "-L", flake_ref(kind, sha), "--"]


# Two-tier cache: in-memory dict + on-disk tab-separated file.
# `_cache_file()` reads XDBG_DRIVER_CACHE lazily so `setup_run_env` can
# install a per-PID path before the first lookup — avoids parallel-run
# corruption on the shared default path.
_mem_cache: dict[str, int] | None = None


def _cache_file() -> Path:
    return Path(
        os.environ.get("XDBG_DRIVER_CACHE")
        or str(Path(os.environ.get("TMPDIR", "/tmp")) / "xdbg-driver-cache")
    )


def _cache_snapshot() -> dict[str, int]:
    global _mem_cache
    if _mem_cache is not None:
        return _mem_cache
    cache: dict[str, int] = {}
    path = _cache_file()
    if path.exists():
        for line in path.read_text().splitlines():
            k, _, v = line.partition("\t")
            if k and v:
                try:
                    cache[k] = int(v)
                except ValueError:
                    pass
    _mem_cache = cache
    return cache


def _cache_set(key: str, value: int) -> None:
    cache = _cache_snapshot()
    cache[key] = value
    path = _cache_file()
    path.parent.mkdir(parents=True, exist_ok=True)
    with open(path, "a") as f:
        f.write(f"{key}\t{value}\n")


def probe(mode: str, kind: str, sha: str) -> tuple[str, str]:
    """Returns (status, stderr) where status in {ok, build-failed, eval-failed}."""
    args = ["nix", "build", "-L"]
    if mode == "dry":
        args.append("--dry-run")
    args.append(flake_ref(kind, sha))
    label = f"probing xdbg@{sha[:7]} ({kind}{', dry' if mode == 'dry' else ''})"
    with with_spinner(label):
        r = subprocess.run(args, cwd=repo_root(), capture_output=True, text=True)
    if r.returncode == 0:
        return ("ok", "")
    if re.search(
        r"builder( for '[^']*')? failed|build of '[^']*' failed|builder failed with exit code",
        r.stderr,
    ):
        return ("build-failed", r.stderr)
    return ("eval-failed", r.stderr)


_help_cache: dict[tuple[str, str], tuple[int, str]] = {}


def _run_help(kind: str, sha: str) -> tuple[int, str]:
    """Run `xdbg --help` for the given build, memoized by (kind, sha).
    HEAD --help can take minutes (cold nix build); multiple supports_flag
    calls would otherwise re-pay that cost per flag."""
    key = (kind, sha)
    if key in _help_cache:
        return _help_cache[key]
    try:
        with with_spinner(f"checking xdbg@{sha[:7]} ({kind}) --help"):
            r = subprocess.run(
                [*xdbg_invocation(kind, sha), "--help"],
                cwd=repo_root(),
                capture_output=True,
                text=True,
                env={**os.environ, **sandbox_env()},
            )
        _help_cache[key] = (r.returncode, r.stdout)
    except (subprocess.SubprocessError, OSError):
        _help_cache[key] = (1, "")
    return _help_cache[key]


def supports_flag(kind: str, sha: str, flag: str) -> bool:
    key = f"{kind}:{sha}:flag:{flag}"
    cached = _cache_snapshot().get(key)
    if cached is not None:
        return cached == 0
    rc, out = _run_help(kind, sha)
    ok = rc == 0 and flag in out
    _cache_set(key, 0 if ok else 1)
    return ok


def supports_subcommand(kind: str, sha: str, sub: str) -> bool:
    key = f"{kind}:{sha}:sub:{sub}"
    cached = _cache_snapshot().get(key)
    if cached is not None:
        return cached == 0
    try:
        with with_spinner(f"checking xdbg@{sha[:7]} {sub} --help"):
            r = subprocess.run(
                [*xdbg_invocation(kind, sha), sub, "--help"],
                cwd=repo_root(),
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                env={**os.environ, **sandbox_env()},
            )
            ok = r.returncode == 0
    except (subprocess.SubprocessError, OSError):
        ok = False
    _cache_set(key, 0 if ok else 1)
    return ok


# ---------- Subprocess: run xdbg ----------


def run_xdbg_info(env_extras: dict[str, str], kind: str, sha: str, *args: str) -> int:
    with gh_group(f"xdbg@{sha[:7]} (info) {' '.join(args)}"):
        env = {**os.environ, **env_extras, **sandbox_env(env_extras)}
        r = subprocess.run([*xdbg_invocation(kind, sha), *args], env=env)
    return r.returncode


def run_xdbg(
    *,
    env_extras: dict[str, str],
    out_dir: str,
    tmp_prefix: str,
    extra_flags: list[str],
    kind: str,
    sha: str,
    args: list[str],
    tee: bool = True,
) -> subprocess.CompletedProcess:
    short = sha[:7]
    try:
        env = {**os.environ, **env_extras, **sandbox_env(env_extras)}
        backend = os.environ.get("BACKEND") or env_extras.get("BACKEND") or "dev"
        raw_xvt = (
            env_extras.get("XVT_XDBG_FLAGS") or os.environ.get("XVT_XDBG_FLAGS") or ""
        )
        xvt_flags = [p for p in raw_xvt.strip().split() if p]
        xtra = [*extra_flags, *xvt_flags]
        if supports_flag(kind, sha, "--trace-openmls-kv"):
            xtra.append("--trace-openmls-kv")
        call_dir = tempfile.mkdtemp(prefix=tmp_prefix, dir=out_dir)
        cmd = [
            *xdbg_invocation(kind, sha),
            *xtra,
            "-b",
            backend,
            "--json",
            "--fail-fast",
            *args,
        ]
        r = subprocess.run(cmd, env=env, cwd=call_dir, capture_output=True, text=True)
        if tee:
            if r.stdout:
                sys.stdout.write(r.stdout)
                sys.stdout.flush()
            if r.stderr:
                sys.stderr.write(r.stderr)
                sys.stderr.flush()
        if r.returncode:
            gh_error(f"xdbg@{short} failed: rc={r.returncode}")
        return r
    except (subprocess.SubprocessError, OSError) as e:
        gh_error(f"xdbg@{short} invocation failed: {e}")
        return subprocess.CompletedProcess(args=[], returncode=1, stdout="", stderr="")


# ---------- Output formatting ----------


def emit_stderr_table(test_name: str, results: list[ResultRow]) -> None:
    table = Table(title=f"{test_name} summary", title_justify="left")
    for col in ("STATUS", "KIND", "SHORT", "SHA", "LABEL"):
        table.add_column(col, no_wrap=True)
    for r in results:
        glyph = STATUS_PLAIN.get(r["status"], f"? {r['status']}")
        display = r.get("label") or r.get("branch") or ""
        table.add_row(glyph, r["kind"], r["short"], r["sha"], display)
    _stderr_console.print(table)


def emit_plan_table(parsed: list[Entry]) -> None:
    table = Table()
    for col in ("ROLE", "SHORT", "SHA", "BRANCH", "LABEL"):
        table.add_column(col, no_wrap=True)
    for e in parsed:
        table.add_row(e.role, e.short, e.sha, e.branch, e.label)
    _stderr_console.print(table)


def emit_plan_step_summary(
    test_name: str, profile: str, sample_size: int, parsed: list[Entry]
) -> None:
    path = os.environ.get("GITHUB_STEP_SUMMARY")
    if not path:
        return
    body = (
        f"## xdbg {test_name} plan ({profile}, sample-size: {sample_size})\n\n"
        "```json\n"
        f"{json.dumps([asdict(e) for e in parsed], indent=2)}\n"
        "```\n"
    )
    with open(path, "a") as f:
        f.write(body)


def write_plan_artifact(parsed: list[Entry]) -> None:
    if os.environ.get("GITHUB_OUTPUT"):
        Path("plan.json").write_text(json.dumps([asdict(e) for e in parsed], indent=2))


def format_summary_json(
    *,
    results: list[ResultRow],
    completed_count: int,
    required_failure: bool,
    nightly_failure: bool,
    lenient_nightlies: bool,
    test_kind: str | None = None,
) -> str:
    payload: dict[str, Any] = {
        "completed_count": completed_count,
        "nightly_failure": int(nightly_failure),
        "required_failure": int(required_failure),
        "lenient_nightlies": int(lenient_nightlies),
    }
    if test_kind:
        payload["test_kind"] = test_kind
    payload["results"] = [
        {
            "status": r["status"],
            "short": r["short"],
            "sha": r["sha"],
            "kind": r["kind"],
            "branch": r.get("branch") or "",
            "label": r.get("label") or "",
        }
        for r in results
    ]
    return json.dumps(payload, indent=2) + "\n"


def count_ndjson_logs(out_dir: str) -> int:
    """Count .json files at depth <=1 below out_dir, excluding summary.json."""
    root = Path(out_dir)
    return sum(
        1
        for p in root.rglob("*.json")
        if p.is_file()
        and p.name != "summary.json"
        and len(p.relative_to(root).parts) <= 2
    )


def emit_github_step_summary(
    *,
    test_name: str,
    results: list[ResultRow],
    out_dir: str,
    completed_count: int,
    required_failure: bool,
    nightly_failure: bool,
    lenient_nightlies: bool,
) -> None:
    path = os.environ.get("GITHUB_STEP_SUMMARY")
    if not path:
        return
    rows = []
    for r in results:
        glyph = STATUS_MD.get(r["status"], f"❓ {r['status']}")
        display = r.get("label") or r.get("branch") or "-"
        rows.append(
            f"| {glyph} | {r['kind']} | `{r['short']}` | {display} | `{r['sha']}` |"
        )
    body = (
        f"## {test_name} results\n\n"
        "| Status | Kind | Short | Label / Branch | SHA |\n"
        "|---|---|---|---|---|\n" + "\n".join(rows) + "\n\n"
        f"**completed_count={completed_count} required_failure={int(required_failure)} "
        f"nightly_failure={int(nightly_failure)} lenient={int(lenient_nightlies)}**\n\n"
        f"**libxmtp NDJSON log files captured: {count_ndjson_logs(out_dir)}** "
        "(download the run artifact to inspect)\n"
    )
    with open(path, "a") as f:
        f.write(body)


def record_not_run_remaining(
    results: list[ResultRow], remaining: list[Entry]
) -> list[ResultRow]:
    return results + [
        {
            "status": "SKIP-NOT-RUN",
            "short": e.short,
            "sha": e.sha,
            "kind": e.kind,
            "branch": e.branch,
            "label": e.label,
        }
        for e in remaining
    ]


def setup_run_env(tmp_prefix: str) -> dict[str, str]:
    """Create per-PID DB root + driver cache. Installs XDBG_DRIVER_CACHE
    into os.environ so the in-process cache lookup honors the per-run path
    (the shared default would otherwise let parallel runs collide)."""
    global _mem_cache
    backend = os.environ.get("BACKEND", "dev")
    tmp = os.environ.get("TMPDIR", "/tmp")
    pid = os.getpid()
    xvt_db = f"{tmp}/{tmp_prefix}db-{pid}"
    driver_cache = f"{tmp}/xdbg-driver-cache-{pid}"
    Path(xvt_db).mkdir(parents=True, exist_ok=True)
    Path(driver_cache).touch()
    os.environ["XDBG_DRIVER_CACHE"] = driver_cache
    _mem_cache = None  # drop stale lookups from the pre-setup default path
    return {
        "BACKEND": backend,
        "XVT_DB_ROOT": xvt_db,
        "XDBG_DRIVER_CACHE": driver_cache,
    }


def base_row(e: Entry) -> ResultRow:
    return {
        "short": e.short,
        "sha": e.sha,
        "kind": e.kind,
        "branch": e.branch,
        "label": e.label,
    }


def finalize(
    *,
    test_name: str,
    results: list[ResultRow],
    completed_count: int,
    required_failure: bool,
    nightly_failure: bool,
    lenient_nightlies: bool,
    out_dir: str,
    test_kind: str | None = None,
    coverage_msg: str,
    required_msg: str,
    nightly_strict_msg: str,
    nightly_lenient_msg: str | None = None,
    success_msg: str,
) -> None:
    """Render the per-run summary table + JSON artifact + step-summary
    markdown + exit policy. Shared by both cross-talk-test and
    cross-version-test runners."""
    emit_stderr_table(test_name, results)
    summary_json = f"{out_dir}/summary.json"
    Path(summary_json).write_text(
        format_summary_json(
            results=results,
            completed_count=completed_count,
            required_failure=required_failure,
            nightly_failure=nightly_failure,
            lenient_nightlies=lenient_nightlies,
            test_kind=test_kind,
        )
    )
    gh_notice(f"summary JSON: {summary_json}")
    emit_github_step_summary(
        test_name=test_name,
        results=results,
        out_dir=out_dir,
        completed_count=completed_count,
        required_failure=required_failure,
        nightly_failure=nightly_failure,
        lenient_nightlies=lenient_nightlies,
    )
    if completed_count < 2:
        gh_error(coverage_msg.format(completed=completed_count))
        sys.exit(1)
    if required_failure:
        gh_error(required_msg)
        sys.exit(1)
    if nightly_failure and not lenient_nightlies:
        gh_error(nightly_strict_msg)
        sys.exit(1)
    if nightly_failure and nightly_lenient_msg:
        gh_notice(nightly_lenient_msg)
    else:
        gh_notice(success_msg)


def _parse_cli(prog: str, argv: list[str]) -> tuple[str, int | None, list[str]]:
    """Returns (profile, sample_size, xdbg_args). Exits 2 on parse error.

    Internal helper to ``prepare_run`` — has policy (sys.exit) baked in.
    """
    p = argparse.ArgumentParser(prog=prog, add_help=False)
    p.add_argument("command", nargs="?")
    p.add_argument("--profile", default="stable", choices=("stable", "nightly"))
    p.add_argument("--sample-size", type=int, default=None)
    p.add_argument("-h", "--help", action="store_true")
    if "--" in argv:
        idx = argv.index("--")
        known, xdbg_args = argv[:idx], argv[idx + 1 :]
    else:
        known, xdbg_args = argv, []
    try:
        ns = p.parse_args(known)
    except SystemExit:
        sys.exit(2)
    if ns.help or ns.command != "run":
        print(
            f"Usage: {prog} run [--profile stable|nightly]\n"
            f"       {' ' * len(prog)}     [--sample-size N] [-- xdbg-flags...]"
        )
        sys.exit(0 if ns.help else 2)
    return ns.profile, ns.sample_size, xdbg_args


def default_sample_size(profile: str) -> int:
    return 0 if profile == "stable" else 3


def prepare_run(
    prog: str, tmp_prefix: str, argv: list[str]
) -> tuple[list[Entry], dict[str, str], str, bool, str, int]:
    """Common setup: parse CLI, fetch + pick plan, set up out_dir + env.

    Returns (parsed_plan, env_extras, out_dir, lenient, profile, sample_size).
    """
    profile, sample_size, xdbg_args = _parse_cli(prog, argv)
    if sample_size is None:
        sample_size = default_sample_size(profile)
    lenient = profile == "nightly"
    n = bootstrap_fetch()
    if n == 0:
        gh_warning("no nightly tags matched; nightly sampling will be empty")
    else:
        eputs(f"Fetched {n} nightly tags")
    parsed = [parse_plan_entry(r) for r in pick_versions(sample_size)]
    if any(validate_entry(e) for e in parsed):
        bad = next(e for e in parsed if validate_entry(e))
        eputs(f"{prog}: plan entry missing sha or role: {asdict(bad)}")
        sys.exit(2)
    if len(parsed) < 2:
        eputs(f"{prog}: plan must have at least 2 entries; got {len(parsed)}")
        sys.exit(2)
    emit_plan_step_summary(prog.replace("-test", ""), profile, sample_size, parsed)
    write_plan_artifact(parsed)
    out_dir = (
        os.environ.get("XVT_OUT_DIR")
        or f"{os.environ.get('TMPDIR', '/tmp')}/{tmp_prefix}out-{os.getpid()}"
    )
    Path(out_dir).mkdir(parents=True, exist_ok=True)
    if gh_out := os.environ.get("GITHUB_OUTPUT"):
        with open(gh_out, "a") as f:
            f.write(f"out_dir={out_dir}\n")
    env_extras = setup_run_env(tmp_prefix)
    xdbg_flags = " ".join(xdbg_args)
    if xdbg_flags:
        env_extras["XVT_XDBG_FLAGS"] = xdbg_flags
        gh_notice(f"{prog} forwarding xdbg flags: {xdbg_flags}")
    gh_notice(f"{prog} XDBG_DB_ROOT={env_extras['XVT_DB_ROOT']}")
    gh_notice(f"{prog} out_dir={out_dir}")
    return parsed, env_extras, out_dir, lenient, profile, sample_size
