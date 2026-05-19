#!/usr/bin/env python3
"""cross-talk-test — drive multiple xdbg builds against a shared
XDBG_DB_ROOT under --strict-versioning so each version's identities
are partitioned. Tests wire-level MLS interop."""

from __future__ import annotations

import json
import sys
from pathlib import Path

import xdbg_driver_lib_py as d


_FINALIZE_MSGS = {
    "coverage_msg": "insufficient cross-talk coverage (completed={completed}); need >=2",
    "required_msg": "required version failed cross-talk healthcheck",
    "nightly_strict_msg": "nightly failure (strict mode)",
    "success_msg": "cross-talk-test completed",
}


def run_strict(env_extras, out_dir, kind, sha, *args, tee=True):
    return d.run_xdbg(
        env_extras=env_extras,
        out_dir=out_dir,
        tmp_prefix="ctt-call-",
        extra_flags=["--strict-versioning"],
        kind=kind,
        sha=sha,
        args=list(args),
        tee=tee,
    )


def preflight(env_extras: dict[str, str], e: d.Entry) -> tuple[str, str]:
    """Returns (status, reason). status in {ok, skip}; on hard failure exits."""
    mode = "full" if e.kind == "nightly" else "dry"
    status, stderr = d.build(mode, e.kind, e.sha)
    if status != "ok":
        if stderr:
            sys.stderr.write(stderr)
            sys.stderr.flush()
        if e.required:
            d.gh_error(f"build failed for {e.short} ({e.kind}) status={status}")
            sys.exit(1)
        d.gh_warning(
            f"xdbg@{e.short} ({e.kind}) build {status}; dropping from cross-talk plan"
        )
        return ("skip", f"build-{status}")
    if not d.supports_flag(e.kind, e.sha, "--strict-versioning"):
        if e.required:
            d.gh_error(
                f"xdbg@{e.short} lacks --strict-versioning; cannot participate in cross-talk-test"
            )
            sys.exit(1)
        d.gh_warning(
            f"xdbg@{e.short} lacks --strict-versioning; dropping from cross-talk plan"
        )
        return ("skip", "lacks-strict-versioning")
    if not d.supports_subcommand(e.kind, e.sha, "sync"):
        if e.required:
            d.gh_error(
                f"xdbg@{e.short} lacks 'sync' subcommand; cannot participate in cross-talk-test"
            )
            sys.exit(1)
        d.gh_warning(
            f"xdbg@{e.short} lacks 'sync' subcommand; dropping from cross-talk plan"
        )
        return ("skip", "lacks-sync")
    d.run_xdbg_info(env_extras, e.kind, e.sha, "--version")
    return ("ok", "")


def run_sequence(
    env_extras: dict[str, str], parsed: list[d.Entry], out_dir: str, lenient: bool
) -> None:
    d.gh_notice(f"Running {len(parsed)} versions")
    d.emit_plan_table(parsed)

    preflight_rows = [(e, *preflight(env_extras, e)) for e in parsed]
    runnable = [e for e, status, _ in preflight_rows if status == "ok"]
    skip_rows: list[d.ResultRow] = [
        {**d.base_row(e), "status": f"SKIP-CAPABILITY-{reason}"}
        for e, status, reason in preflight_rows
        if status == "skip"
    ]
    nightly_skip = bool(skip_rows)

    if not runnable:
        d.finalize(
            test_name="cross-talk-test",
            test_kind="cross-talk",
            results=skip_rows,
            completed_count=0,
            required_failure=False,
            nightly_failure=nightly_skip,
            lenient_nightlies=lenient,
            out_dir=out_dir,
            **_FINALIZE_MSGS,
        )
        return

    oldest = runnable[0]
    with d.gh_group("cross-talk phase 1: bootstrap identities"):
        for e in runnable:
            run_strict(
                env_extras,
                out_dir,
                e.kind,
                e.sha,
                "generate",
                "-e",
                "identity",
                "--amount",
                "1",
            )

    with d.gh_group("cross-talk phase 2: oldest creates group"):
        run_strict(
            env_extras,
            out_dir,
            oldest.kind,
            oldest.sha,
            "generate",
            "-e",
            "group",
            "--amount",
            "1",
        )
        # Use --out so xdbg writes the export JSON to a file rather than
        # stdout — `-vvvvv` floods stdout with JSON log lines that would
        # otherwise mix with the payload and break parsing.
        export_path = Path(out_dir) / "groups-export.json"
        r = run_strict(
            env_extras,
            out_dir,
            oldest.kind,
            oldest.sha,
            "export",
            "-e",
            "group",
            "--out",
            str(export_path),
            tee=False,
        )
        gid = None
        if r.returncode == 0 and export_path.exists():
            try:
                groups = json.loads(export_path.read_text())
                if groups:
                    gid = groups[0].get("id")
            except json.JSONDecodeError:
                pass
        if not gid:
            d.gh_error(
                "could not capture group_id from `xdbg export -e group`; aborting"
            )
            sys.exit(1)
        d.gh_notice(f"shared group_id={gid}")

    with d.gh_group("cross-talk phase 3: oldest adds joiner identities + promotes"):
        run_strict(
            env_extras,
            out_dir,
            oldest.kind,
            oldest.sha,
            "modify",
            "add-from-redb",
            gid,
            "--include-versions",
            "other",
            "--promote-super-admin",
        )

    with d.gh_group("cross-talk sync all clients"):
        for e in runnable:
            run_strict(env_extras, out_dir, e.kind, e.sha, "sync")

    with d.gh_group("cross-talk phase 4: healthcheck"):
        results: list[d.ResultRow] = []
        completed = 0
        required_fail = False
        nightly_fail = False
        for e in runnable:
            row = d.base_row(e)
            r = run_strict(env_extras, out_dir, e.kind, e.sha, "healthcheck")
            if r.returncode == 0:
                results.append({**row, "status": "PASS"})
                completed += 1
            else:
                results.append({**row, "status": "FAIL"})
                if e.kind == "nightly":
                    nightly_fail = True
                else:
                    required_fail = True

    d.finalize(
        test_name="cross-talk-test",
        test_kind="cross-talk",
        results=skip_rows + results,
        completed_count=completed,
        required_failure=required_fail,
        nightly_failure=nightly_fail or nightly_skip,
        lenient_nightlies=lenient,
        out_dir=out_dir,
        **_FINALIZE_MSGS,
    )


def main() -> None:
    parsed, env_extras, out_dir, lenient, _, _ = d.prepare_run(
        "cross-talk-test", "ctt-", sys.argv[1:]
    )
    run_sequence(env_extras, parsed, out_dir, lenient)


if __name__ == "__main__":
    main()
