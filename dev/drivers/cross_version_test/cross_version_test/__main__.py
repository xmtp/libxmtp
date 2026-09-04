#!/usr/bin/env python3
"""cross-version-test — drive multiple xdbg builds against a shared
XVT_DB_ROOT to detect cross-version MLS regressions."""

from __future__ import annotations

import sys

import xdbg_driver_lib_py as d


def run_sequence(
    env_extras: dict[str, str], parsed: list[d.Entry], out_dir: str, lenient: bool
) -> None:
    d.gh_notice(f"Running {len(parsed)} versions")
    d.emit_plan_table(parsed)

    results: list[d.ResultRow] = []
    completed = 0
    required_fail = False
    nightly_fail = False

    for i, e in enumerate(parsed):
        row = d.base_row(e)
        build_mode = "dry" if e.required else "full"
        status, stderr = d.build(build_mode, e.kind, e.sha)
        rest = parsed[i + 1 :]

        if status == "build-failed":
            if stderr:
                sys.stderr.write(stderr)
                sys.stderr.flush()
            if e.required:
                d.gh_error(
                    f"run-sequence: required version {e.short} ({e.branch}) fails to build"
                )
                results = d.record_not_run_remaining(
                    results + [{**row, "status": "FAIL"}], rest
                )
                required_fail = True
                break
            d.gh_warning(f"xdbg@{e.short} nightly {e.label} fails to build; skipping")
            results.append({**row, "status": "SKIP-BUILD"})
            continue

        if status == "eval-failed":
            if stderr:
                sys.stderr.write(stderr)
                sys.stderr.flush()
            if e.required:
                d.gh_error(
                    f"run-sequence: aborting on build eval failure for {e.short}"
                )
                results = d.record_not_run_remaining(
                    results + [{**row, "status": "FAIL"}], rest
                )
                required_fail = True
                break
            d.gh_warning(
                f"xdbg@{e.short} nightly {e.label} build eval failed; skipping"
            )
            results.append({**row, "status": "SKIP-EVAL"})
            nightly_fail = True
            continue

        # status == "ok"
        d.run_xdbg_info(env_extras, e.kind, e.sha, "--version")
        if not d.supports_subcommand(e.kind, e.sha, "healthcheck"):
            if e.required:
                d.gh_error(
                    f"xdbg@{e.short} required version ({e.branch}) lacks 'healthcheck' subcommand; "
                    "cannot continue"
                )
                results = d.record_not_run_remaining(
                    results + [{**row, "status": "FAIL"}], rest
                )
                required_fail = True
                break
            d.gh_warning(
                f"xdbg@{e.short} nightly {e.label} lacks 'healthcheck' subcommand; skipping"
            )
            results.append({**row, "status": "SKIP-NO-HEALTHCHECK"})
            continue

        with d.gh_group(f"healthcheck@{e.short} ({e.kind} {e.label})"):
            r = d.run_xdbg(
                env_extras=env_extras,
                out_dir=out_dir,
                tmp_prefix="xvt-call-",
                extra_flags=[],
                kind=e.kind,
                sha=e.sha,
                args=["healthcheck"],
            )
        if r.returncode == 0:
            results.append({**row, "status": "PASS"})
            completed += 1
        else:
            results.append({**row, "status": "FAIL"})
            if e.required:
                d.gh_error(
                    f"xdbg@{e.short} required version ({e.kind}) healthcheck failure "
                    f"(rc={r.returncode}); continuing to remaining entries"
                )
                required_fail = True
            else:
                d.gh_warning(
                    f"xdbg@{e.short} nightly {e.label} healthcheck failure "
                    f"(rc={r.returncode}); continuing so later versions (incl. HEAD) still run"
                )
                nightly_fail = True

    d.finalize(
        test_name="cross-version-test",
        results=results,
        completed_count=completed,
        required_failure=required_fail,
        nightly_failure=nightly_fail,
        lenient_nightlies=lenient,
        out_dir=out_dir,
        coverage_msg="run-sequence: insufficient cross-version coverage (completed={completed}); "
        "need at least 2 successful healthcheck runs",
        required_msg="run-sequence: one or more required versions failed",
        nightly_strict_msg="run-sequence: one or more nightlies failed (strict mode)",
        nightly_lenient_msg="run-sequence completed with nightly failures (lenient mode; "
        "HEAD + required versions OK)",
        success_msg="run-sequence completed without failures",
    )


def main() -> None:
    parsed, env_extras, out_dir, lenient, _, _ = d.prepare_run(
        "cross-version-test", "xvt-", sys.argv[1:]
    )
    run_sequence(env_extras, parsed, out_dir, lenient)


if __name__ == "__main__":
    main()
