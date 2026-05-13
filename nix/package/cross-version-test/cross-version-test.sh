#!/usr/bin/env bash
# cross-version-test — drive multiple xdbg builds against a shared
# XVT_DB_ROOT to detect cross-version MLS regressions.
#
# Subcommands:
#   pick-versions [--sample-size N]
#       Emit plan JSON to stdout.
#   run-sequence [--lenient-nightlies] <plan.json>
#       Execute the plan; exits non-zero on first failure. Stable
#       branches + HEAD are always strict. With --lenient-nightlies a
#       nightly runtime failure emits ::warning:: and the sequence
#       continues instead of failing.
#
# See docs/superpowers/specs/2026-05-08-xdbg-cross-version-compat-design.md
# (filename retained for history; spec describes this same tool).
set -euo pipefail

usage() {
    cat <<'EOF'
Usage:
  cross-version-test pick-versions [--sample-size N]
  cross-version-test run-sequence [--lenient-nightlies] <plan.json>
  cross-version-test --help
EOF
}

main() {
    if [ $# -eq 0 ]; then
        usage >&2
        exit 2
    fi

    case "$1" in
        --help|-h)
            usage
            ;;
        pick-versions)
            shift
            cmd_pick_versions "$@"
            ;;
        run-sequence)
            shift
            cmd_run_sequence "$@"
            ;;
        *)
            echo "unknown subcommand: $1" >&2
            usage >&2
            exit 2
            ;;
    esac
}

# Print the names of the last two stable release branches (newest first),
# one per line. Names are emitted as the suffix after 'release/' — e.g.
# '1.10.0', 'v1.9'.
#
# Resolution rules (per spec):
#   - source: refs/remotes/origin/release/*
#   - keep names matching ^v?[0-9]+\.[0-9]+(\.[0-9]+)?$
#   - sort by semver after stripping leading 'v'
#   - dedup by major.minor (newest in each minor wins)
last_two_stable_branches() {
    git for-each-ref --format='%(refname:short)' \
        'refs/remotes/origin/release/*' \
        | sed -n 's|^origin/release/||p' \
        | awk '
            {
                name = $0
                ver = name
                sub(/^v/, "", ver)
                if (ver !~ /^[0-9]+\.[0-9]+(\.[0-9]+)?$/) next
                n = split(ver, parts, ".")
                major = parts[1]; minor = parts[2]
                patch = (n >= 3) ? parts[3] : 0
                printf "%010d.%010d.%010d\t%s\t%s\n", major, minor, patch, major"."minor, name
            }' \
        | sort -k1,1 \
        | awk -F'\t' '
            # Dedup by major.minor; last-write-wins because input is sorted
            # ascending — the highest patch per minor overwrites earlier ones.
            { rows[$2] = $0 }
            END {
                for (k in rows) print rows[k]
            }' \
        | sort -k1,1 \
        | tail -n 2 \
        | awk -F'\t' '{ print $3 }' \
        | tac
}

# Print unique nightly-tag (date, sha7) pairs, newest first, in the format
# "YYYYMMDD<TAB>sha7". Tag scheme:
#   <binding>-X.Y.Z-nightly.YYYYMMDD.<sha7>
# Each nightly date produces multiple tags (one per binding family); we
# dedupe on the (date, sha7) suffix so each calendar day contributes at
# most one candidate sha.
nightly_tag_candidates() {
    git tag --sort=-creatordate \
        | grep -E -- '-nightly\.[0-9]{8}\.[0-9a-f]{7}$' \
        | sed -E 's/.*-nightly\.([0-9]{8})\.([0-9a-f]{7})$/\1\t\2/' \
        | awk -F'\t' '!seen[$1"\t"$2]++'
}

cmd_pick_versions() {
    local sample_size=3
    while [ $# -gt 0 ]; do
        case "$1" in
            --sample-size)
                sample_size="${2:?--sample-size requires an argument}"
                shift 2
                ;;
            *)
                echo "pick-versions: unknown arg: $1" >&2
                exit 2
                ;;
        esac
    done

    if ! [[ "$sample_size" =~ ^[0-9]+$ ]]; then
        echo "pick-versions: --sample-size must be a non-negative integer, got '$sample_size'" >&2
        exit 2
    fi

    local -a BRANCHES
    mapfile -t BRANCHES < <(last_two_stable_branches)
    if [ "${#BRANCHES[@]}" -lt 2 ]; then
        echo "pick-versions: need at least 2 stable release branches; found ${#BRANCHES[@]}" >&2
        exit 1
    fi

    # BRANCHES[0] is newest (e.g. 1.10.0), BRANCHES[1] next (e.g. v1.9).
    local newest_branch="${BRANCHES[0]}"
    local next_branch="${BRANCHES[1]}"

    local newest_sha next_sha head_sha
    {
        read -r newest_sha
        read -r next_sha
        read -r head_sha
    } < <(git rev-parse \
              "origin/release/${newest_branch}" \
              "origin/release/${next_branch}" \
              HEAD)

    # Always include the two stable branch heads + HEAD; add $sample_size
    # random nightly tags. Nightlies are tagged daily off main, so each
    # picked sha7 resolves to a real commit reachable from main without
    # walking git log. sample_size=0 reduces to the 3-version base set.
    #
    # HEAD's label embeds the current ref name so locally-run plans show
    # something more useful than empty (e.g. `HEAD@feature-branch`). In CI
    # the checkout is detached so `--abbrev-ref` returns `HEAD`; we elide
    # the redundant suffix in that case.
    local head_ref head_label="HEAD"
    head_ref=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "HEAD")
    if [ -n "$head_ref" ] && [ "$head_ref" != "HEAD" ]; then
        head_label="HEAD@${head_ref}"
    fi

    local -a shas=("$newest_sha" "$next_sha" "$head_sha")
    local -a branches_for_sha=("release/${newest_branch}" "release/${next_branch}" "")
    local -a labels_for_sha=("" "" "$head_label")

    if [ "$sample_size" -gt 0 ]; then
        # nightly_tag_candidates emits newest-first; `head -n` takes the
        # most recent N. Deterministic = easier triage than `shuf -n`.
        local picked
        picked=$(nightly_tag_candidates | head -n "$sample_size") || true
        if [ -n "$picked" ]; then
            local date sha7 full
            while IFS=$'\t' read -r date sha7; do
                # Resolve sha7 to a full sha. Fails loudly with a labelled
                # message if the short sha is ambiguous (multiple matches)
                # or missing (e.g. tag points at a pruned/gc'd commit).
                if ! full=$(git rev-parse --verify "$sha7^{commit}" 2>/dev/null); then
                    echo "pick-versions: failed to resolve nightly sha7 '$sha7' (ambiguous or missing); skipping nightly.${date}" >&2
                    continue
                fi
                shas+=("$full")
                branches_for_sha+=("")
                labels_for_sha+=("nightly.${date}")
            done <<< "$picked"
        fi
    fi

    # Dedup on sha (a sampled nightly could collide with a stable branch
    # HEAD or with HEAD itself). Then sort oldest -> newest by commit time.
    declare -A seen=()
    local -a uniq_shas=()
    local -a uniq_branches=()
    local -a uniq_labels=()
    local i
    for i in "${!shas[@]}"; do
        local s="${shas[$i]}"
        if [ -z "${seen[$s]+x}" ]; then
            seen[$s]=1
            uniq_shas+=("$s")
            uniq_branches+=("${branches_for_sha[$i]}")
            uniq_labels+=("${labels_for_sha[$i]}")
        fi
    done

    # Order by semver, grouped: stable branches first (asc), then nightlies
    # (asc by YYYYMMDD), then HEAD last. Deterministic across clones and
    # ensures the creator always runs before any sender. HEAD runs last so
    # every older version has already exercised the shared state by the
    # time HEAD reads it — that's the highest-signal regression check.
    #
    # Sort key (tab-separated, lexical sort with zero-padded components):
    #   kind_key (0=stable, 1=nightly, 2=head)
    #   semver_key (10-digit zero-padded major.minor.patch, or YYYYMMDD)
    # The kind_key dominates so groups stay contiguous regardless of the
    # nightly's nominal X.Y.Z prefix (today's nightlies advertise 1.10.0
    # but their build sha is well past release/1.10.0).
    local sorted_payload
    sorted_payload=$(
        for i in "${!uniq_shas[@]}"; do
            local _kind_key=2 _semver_key="0000000000.0000000000.0000000000"
            local _label="${uniq_labels[$i]}"
            local _branch="${uniq_branches[$i]}"
            if [ -n "$_label" ]; then
                # nightly.YYYYMMDD → key = YYYYMMDD (already zero-padded)
                _kind_key=1
                _semver_key="${_label#nightly.}"
            elif [ -n "$_branch" ]; then
                # release/v?X.Y(.Z)? → key = padded major.minor.patch
                _kind_key=0
                local _ver="${_branch#release/}"
                _ver="${_ver#v}"
                local _maj _min _pat
                local -a _parts
                IFS='.' read -ra _parts <<< "$_ver"
                _maj="${_parts[0]:-0}"
                _min="${_parts[1]:-0}"
                _pat="${_parts[2]:-0}"
                _semver_key=$(printf '%010d.%010d.%010d' "$_maj" "$_min" "$_pat")
            fi
            printf '%s\t%s\t%s\t%s\t%s\n' \
                "$_kind_key" \
                "$_semver_key" \
                "${uniq_shas[$i]}" \
                "$_branch" \
                "$_label"
        done | sort -k1,1n -k2,2 | cut -f2-
    )

    # Emit JSON. Role = creator for the first REQUIRED row (stable branch
    # HEAD or repo HEAD, i.e. label empty), sender for the rest. Nightlies
    # can never be the planned creator: a failing nightly must not take
    # down the sequence; the creator is load-bearing.
    # Fields: sha, short (7-char prefix), role, branch (release/X if a
    # stable branch HEAD, empty otherwise), label (nightly.YYYYMMDD if
    # the row came from a nightly tag, empty otherwise).
    echo "$sorted_payload" \
        | awk -F'\t' '
            BEGIN { print "["; creator_seen = 0 }
            {
                if (NR > 1) printf ",\n"
                is_required = ($4 == "")
                if (!creator_seen && is_required) {
                    role = "creator"
                    creator_seen = 1
                } else {
                    role = "sender"
                }
                short = substr($2, 1, 7)
                printf "  {\"sha\":\"%s\",\"short\":\"%s\",\"role\":\"%s\",\"branch\":\"%s\",\"label\":\"%s\"}",
                       $2, short, role, $3, $4
            }
            END { print "\n]" }'
}

# Build the flake reference for a (kind, sha) pair.
#
# For kind=head the local working-tree flake (`.#xdbg`) is used, so:
#   - CI doesn't need HEAD pushed to github before run-sequence executes
#     (it always is in practice, but this avoids a 404 race),
#   - local `just cross-test` works against unpushed working-copy commits.
# All other kinds (stable, nightly) resolve via github:xmtp/libxmtp/<sha>.
xdbg_flake_ref() {
    local kind="$1" sha="$2"
    if [ "$kind" = "head" ]; then
        printf '.#xdbg'
    else
        printf 'github:xmtp/libxmtp/%s#xdbg' "$sha"
    fi
}

# Resolve the path to the built xdbg binary for a (kind, sha) pair.
#
# Uses `nix build --no-link --print-out-paths` so we get the store path
# without leaving a `result` symlink in CWD. Caller is expected to have
# already invoked xdbg_probe_available "full" (or accept the build cost
# here). Echoes the absolute path of `xdbg` binary on stdout, returns
# non-zero on any failure.
xdbg_binary_path() {
    local kind="$1" sha="$2"
    local out_path
    out_path=$(nix build --no-link --print-out-paths "$(xdbg_flake_ref "$kind" "$sha")" 2>&1) || {
        printf '%s\n' "$out_path" >&2
        return 1
    }
    # nix may print multiple lines; binary is at $out_path/bin/xdbg.
    out_path=$(printf '%s\n' "$out_path" | tail -n1)
    if [ ! -x "$out_path/bin/xdbg" ]; then
        echo "xdbg_binary_path: no xdbg binary at $out_path/bin/xdbg" >&2
        return 1
    fi
    printf '%s\n' "$out_path/bin/xdbg"
}

# Compose the env-prefix args that point xdbg at the shared XDBG_DB_ROOT
# for an invocation. Echoes the args one per line for use with bash arrays.
# Globals read: XVT_DB_ROOT.
xdbg_sandbox_env_args() {
    printf 'XDBG_DB_ROOT=%s\n' "$XVT_DB_ROOT"
}

# Probe whether xdbg is exposed and (optionally) builds for this entry.
#
# Args: mode kind sha
#   mode=dry  → `nix build --dry-run` (evaluates, doesn't build).
#   mode=full → `nix build` (compiles xdbg). Used for entries that the
#               caller wants to verify CAN build, so a downstream
#               failure can be classified as compile-error.
#   kind      → "head" uses local .#xdbg, else uses github:xmtp/libxmtp/<sha>.
#
# Returns:
#   0 — attribute present (and built, if mode=full).
#   2 — flake evaluation failure (network / eval regression / broken pin /
#       attribute missing). Caller treats as fatal: silently skipping
#       would let a real flake regression sneak past CI.
#   3 — flake evaluates fine, but the xdbg derivation fails to build
#       (compile error, broken interface vs current deps). Caller decides
#       whether to skip (nightly) or fail (required entry).
xdbg_probe_available() {
    local mode="$1" kind="$2" sha="$3"
    local short="${sha:0:7}"
    local probe_out probe_rc=0
    local nix_args=("$(xdbg_flake_ref "$kind" "$sha")")
    if [ "$mode" = "dry" ]; then
        nix_args=(--dry-run "${nix_args[@]}")
    elif [ "$mode" != "full" ]; then
        echo "xdbg_probe_available: bad mode '$mode' (want dry|full)" >&2
        return 2
    fi

    probe_out=$(nix build "${nix_args[@]}" 2>&1) || probe_rc=$?
    if [ $probe_rc -eq 0 ]; then
        return 0
    fi
    # Distinguish eval failure (no derivation produced, or attribute
    # missing) from build failure (derivation built but compile/check
    # failed). nix emits "builder failed with exit code" or
    # "error: build of '...' failed" for build failures.
    if printf '%s\n' "$probe_out" \
            | grep -qE "builder( for '.*')? failed|build of '.*' failed|builder failed with exit code"; then
        printf '%s\n' "$probe_out" >&2
        echo "::warning::xdbg@${short} build failed (compile error or similar)" >&2
        return 3
    fi
    echo "::error::xdbg@${short} probe failed (rc=$probe_rc); flake evaluation broken" >&2
    printf '%s\n' "$probe_out" >&2
    return 2
}

# Per-binary cache of which optional flags an xdbg build supports.
# Key: "$kind:$sha:$flag". Value: 0 (supported), 1 (absent). Populated
# lazily by xdbg_supports_flag via a single `--help` probe per binary.
declare -A XDBG_FLAG_CACHE=()

# Check whether the xdbg binary for ($kind, $sha) advertises $flag in its
# top-level --help output. Returns 0 if present, 1 otherwise.
# Cached so repeated calls (one per xdbg invocation in run-sequence) don't
# re-exec the binary.
xdbg_supports_flag() {
    local kind="$1" sha="$2" flag="$3"
    local key="${kind}:${sha}:${flag}"
    if [ -n "${XDBG_FLAG_CACHE[$key]:-}" ]; then
        return "${XDBG_FLAG_CACHE[$key]}"
    fi
    local bin help_out rc=0
    bin=$(xdbg_binary_path "$kind" "$sha") || {
        XDBG_FLAG_CACHE[$key]=1
        return 1
    }
    local -a env_args
    mapfile -t env_args < <(xdbg_sandbox_env_args)
    help_out=$(env "${env_args[@]}" "$bin" --help 2>&1) || rc=$?
    if [ "$rc" -eq 0 ] && grep -qF -- "$flag" <<<"$help_out"; then
        XDBG_FLAG_CACHE[$key]=0
        return 0
    fi
    XDBG_FLAG_CACHE[$key]=1
    return 1
}

# Probe whether `xdbg <subcommand> --help` succeeds at this kind/sha.
# Used to require `healthcheck` on every tested version. Cached.
# Args: kind sha subcommand
# Returns 0 if supported, 1 otherwise.
xdbg_supports_subcommand() {
    local kind="$1" sha="$2" sub="$3"
    local key="${kind}:${sha}:sub:${sub}"
    if [ -n "${XDBG_FLAG_CACHE[$key]:-}" ]; then
        return "${XDBG_FLAG_CACHE[$key]}"
    fi
    local bin rc=0
    bin=$(xdbg_binary_path "$kind" "$sha") || {
        XDBG_FLAG_CACHE[$key]=1
        return 1
    }
    local -a env_args
    mapfile -t env_args < <(xdbg_sandbox_env_args)
    env "${env_args[@]}" "$bin" "$sub" --help >/dev/null 2>&1 || rc=$?
    if [ "$rc" -eq 0 ]; then
        XDBG_FLAG_CACHE[$key]=0
        return 0
    fi
    XDBG_FLAG_CACHE[$key]=1
    return 1
}

# Run a non-informative xdbg command with `--json --fail-fast`. Every
# tested sha now carries the --fail-fast flag, so non-zero rc IS the
# failure signal — no log-file scanning needed.
#
# Args: base_dir kind sha cmd_args...
# base_dir is the parent under which per-call subdirs are minted so xdbg's
# second-precision log filenames can't collide between rapid invocations.
# kind selects local-flake vs github-flake via xdbg_flake_ref.
#
# The built xdbg binary is exec'd via `env XDBG_DB_ROOT=$XVT_DB_ROOT` so
# every tested version shares one redb/sqlite.
#
# Globals read: BACKEND, XVT_DB_ROOT.
# Echoes combined stdout+stderr; returns 0 on success, non-zero on failure.
run_xdbg_real() {
    local base_dir="$1" kind="$2" sha="$3"; shift 3
    local short="${sha:0:7}"
    local bin out rc
    bin=$(xdbg_binary_path "$kind" "$sha") || {
        echo "::error::xdbg@${short} could not resolve binary path" >&2
        return 1
    }

    local -a env_args
    mapfile -t env_args < <(xdbg_sandbox_env_args)

    # Caller-supplied global flags (verbosity etc.) splice in BEFORE the
    # harness's own `-b ... --json --fail-fast`. Space-separated; empty by
    # default. Useful for `just cross-test -vvvv` style debugging.
    local -a xtra_flags=()
    if [ -n "${XVT_XDBG_FLAGS:-}" ]; then
        # shellcheck disable=SC2206  # word-split is the intent
        xtra_flags=(${XVT_XDBG_FLAGS})
    fi
    # `--trace-openmls-kv` lands on xdbg after the openmls_kv instrumentation
    # PR. Probe --help once per binary so newer stables/nightlies pick it up
    # without code changes here, but older builds don't get rejected.
    if xdbg_supports_flag "$kind" "$sha" "--trace-openmls-kv"; then
        xtra_flags+=(--trace-openmls-kv)
    fi

    local call_dir
    call_dir=$(mktemp -d "${base_dir}/xvt-call-XXXXXX")

    # `|| rc=$?` keeps `set -e` from firing on the xdbg failure (we need
    # to inspect rc + output). `set +e`/`set -e` inside a function leaks
    # the option change to the caller and breaks loops.
    rc=0
    out=$(cd "$call_dir" && env "${env_args[@]}" "$bin" \
            "${xtra_flags[@]}" -b "$BACKEND" --json --fail-fast "$@" 2>&1) || rc=$?

    printf '%s\n' "$out"
    if [ $rc -ne 0 ]; then
        echo "::error::xdbg@${short} failed: rc=$rc" >&2
        return 1
    fi
    return 0
}

# Run an informative xdbg command (e.g. --version) without --fail-fast/--json.
# Same sandboxed-env mechanism as run_xdbg_real.
# Args: kind sha cmd_args...
run_xdbg_info() {
    local kind="$1" sha="$2"; shift 2
    local short="${sha:0:7}"
    echo "::group::xdbg@${short} (info) $*" >&2
    local rc=0
    local bin
    bin=$(xdbg_binary_path "$kind" "$sha") || {
        echo "::endgroup::" >&2
        echo "::error::xdbg@${short} could not resolve binary path" >&2
        return 1
    }
    local -a env_args
    mapfile -t env_args < <(xdbg_sandbox_env_args)
    env "${env_args[@]}" "$bin" "$@" || rc=$?
    echo "::endgroup::" >&2
    return "$rc"
}

cmd_run_sequence() {
    local lenient_nightlies=0
    while [ $# -gt 0 ] && [[ "$1" == --* ]]; do
        case "$1" in
            --lenient-nightlies)
                lenient_nightlies=1
                shift
                ;;
            --)
                shift
                break
                ;;
            *)
                echo "run-sequence: unknown flag: $1" >&2
                exit 2
                ;;
        esac
    done
    local plan="${1:?run-sequence requires a plan.json path}"

    # Export out_dir to GITHUB_OUTPUT BEFORE any validation so the workflow's
    # artifact-upload glob always has a valid prefix even on early exit.
    # Otherwise it expands to /**/*.json and sweeps unrelated checkout files.
    local out_dir="${XVT_OUT_DIR:-${TMPDIR:-/tmp}/xvt-out-$$}"
    mkdir -p "$out_dir"
    if [ -n "${GITHUB_OUTPUT:-}" ]; then
        echo "out_dir=$out_dir" >> "$GITHUB_OUTPUT"
    fi

    if [ ! -r "$plan" ]; then
        echo "run-sequence: cannot read plan: $plan" >&2
        exit 2
    fi

    : "${BACKEND:=dev}"
    export BACKEND

    # Shared xdbg data dir across all tested versions. release/v1.9 +
    # release/1.10.0 both carry the XDBG_DB_ROOT backport, and HEAD has
    # it natively, so a single env var unifies state.
    XVT_DB_ROOT="${TMPDIR:-/tmp}/xvt-db-$$"
    mkdir -p "$XVT_DB_ROOT"
    export XVT_DB_ROOT
    echo "::notice::xvt XDBG_DB_ROOT=$XVT_DB_ROOT" >&2
    echo "::notice::xvt output dir: $out_dir" >&2

    # One jq pass: first line is length, then (role|short|sha|branch|label)
    # per plan entry. Uses `|` instead of tab because bash's `read` with
    # IFS=$'\t' treats tabs as whitespace and collapses consecutive ones,
    # losing empty-branch entries (nightlies have empty branch + non-empty
    # label). `|` is non-whitespace so consecutive separators preserve
    # empty fields. Capture jq output into a variable first so jq failures
    # propagate — `mapfile < <(jq …)` swallows jq's exit status via the
    # process substitution and leaves plan_lines empty without erroring.
    local jq_out jq_rc=0
    jq_out=$(jq -r 'length, (.[] | [.role, .short, .sha, (.branch // ""), (.label // "")] | join("|"))' "$plan") || jq_rc=$?
    if [ "$jq_rc" -ne 0 ]; then
        echo "run-sequence: failed to parse plan: $plan" >&2
        exit 2
    fi
    local -a plan_lines
    mapfile -t plan_lines <<< "$jq_out"
    local n="${plan_lines[0]}"
    if [ "$n" -lt 2 ]; then
        echo "run-sequence: plan must have at least 2 entries; got $n" >&2
        exit 2
    fi
    echo "::notice::Running ${n} versions" >&2

    {
        printf 'ROLE|SHORT|SHA|BRANCH|LABEL\n'
        printf '%s\n' "${plan_lines[@]:1}"
    } | column -t -s '|' >&2

    # Per-version status accumulation. Each entry:
    #   "STATUS|short|sha|kind|branch|label"
    # STATUS values:
    #   PASS         — ran successfully
    #   FAIL         — runtime failure (or build failure on required entry)
    #   SKIP-BUILD   — nightly failed to compile; skipped
    #   SKIP-NOT-RUN — planned but the loop bailed early on an earlier
    #                  required-version failure
    #   SKIP         — generic skip (reserved)
    # kind: stable | nightly | head
    local -a results=()
    local completed_count=0
    local nightly_failure=0
    local required_failure=0
    local _record_not_run_remaining=0
    local sha role probe_rc probe_mode kind
    local entry _short branch label
    local plan_idx=0
    for entry in "${plan_lines[@]:1}"; do
        plan_idx=$((plan_idx + 1))
        IFS='|' read -r role _short sha branch label <<< "$entry"
        if [ -z "$sha" ] || [ -z "$role" ] || [ "$sha" = "null" ] || [ "$role" = "null" ]; then
            echo "run-sequence: plan entry missing sha or role" >&2
            exit 2
        fi

        # Classify entry by label/branch:
        #   label starts with "nightly." → nightly
        #   label starts with "HEAD"     → head (carries ref name like HEAD@<branch>)
        #   branch non-empty              → stable release branch
        #   neither                       → head (legacy: empty-label HEAD)
        case "$label" in
            nightly.*) kind="nightly" ;;
            HEAD*)     kind="head" ;;
            "")
                if [ -n "$branch" ]; then
                    kind="stable"
                else
                    kind="head"
                fi
                ;;
            *)
                echo "::warning::unrecognized plan label '$label'; treating as nightly" >&2
                kind="nightly"
                ;;
        esac
        local is_required=1
        if [ "$kind" = "nightly" ]; then
            is_required=0
        fi

        # Required entries: probe dry (fast). Nightlies: probe full so a
        # compile failure surfaces here and can be skipped without polluting
        # the sequence; required entries that fail to build are fatal.
        if [ "$is_required" -eq 1 ]; then
            probe_mode=dry
        else
            probe_mode=full
        fi

        probe_rc=0
        xdbg_probe_available "$probe_mode" "$kind" "$sha" || probe_rc=$?
        case "$probe_rc" in
            0) ;;                                        # present (built if full)
            3)
                if [ "$is_required" -eq 0 ]; then
                    echo "::warning::xdbg@${sha:0:7} nightly $label fails to build; skipping" >&2
                    results+=("SKIP-BUILD|${sha:0:7}|$sha|$kind|$branch|$label")
                    nightly_failure=1
                    continue
                fi
                echo "::error::run-sequence: required version ${sha:0:7} ($branch) fails to build" >&2
                results+=("FAIL|${sha:0:7}|$sha|$kind|$branch|$label")
                required_failure=1
                # Required build failure is fatal; record remaining entries
                # as NOT-RUN so the summary still lists every planned version.
                _record_not_run_remaining=1
                break
                ;;
            *)
                echo "::error::run-sequence: aborting on probe failure for ${sha:0:7} (rc=$probe_rc)" >&2
                results+=("FAIL|${sha:0:7}|$sha|$kind|$branch|$label")
                required_failure=1
                _record_not_run_remaining=1
                break
                ;;
        esac

        run_xdbg_info "$kind" "$sha" --version || true

        # Require the `healthcheck` subcommand on every tested version.
        # Versions without it cannot drive the cross-version sequence —
        # the old `generate identity/group/message` path is gone.
        if ! xdbg_supports_subcommand "$kind" "$sha" healthcheck; then
            if [ "$is_required" -eq 0 ]; then
                echo "::warning::xdbg@${sha:0:7} nightly $label lacks 'healthcheck' subcommand; skipping" >&2
                results+=("SKIP-NO-HEALTHCHECK|${sha:0:7}|$sha|$kind|$branch|$label")
                nightly_failure=1
                continue
            fi
            echo "::error::xdbg@${sha:0:7} required version ($branch) lacks 'healthcheck' subcommand; cannot continue" >&2
            results+=("FAIL|${sha:0:7}|$sha|$kind|$branch|$label")
            required_failure=1
            _record_not_run_remaining=1
            break
        fi

        local entry_rc=0
        echo "::group::healthcheck@${sha:0:7} ($kind $label)" >&2
        run_xdbg_real "$out_dir" "$kind" "$sha" healthcheck || entry_rc=$?
        echo "::endgroup::" >&2

        if [ "$entry_rc" -eq 0 ]; then
            completed_count=$((completed_count + 1))
            results+=("PASS|${sha:0:7}|$sha|$kind|$branch|$label")
        else
            results+=("FAIL|${sha:0:7}|$sha|$kind|$branch|$label")
            if [ "$is_required" -eq 0 ]; then
                echo "::warning::xdbg@${sha:0:7} nightly $label healthcheck failure (rc=$entry_rc); continuing so later versions (incl. HEAD) still run" >&2
                nightly_failure=1
                continue
            fi
            echo "::error::xdbg@${sha:0:7} required version ($kind) healthcheck failure (rc=$entry_rc); continuing to remaining entries" >&2
            required_failure=1
        fi
    done

    # If we broke out of the loop early (required-version fatal), record
    # the remaining planned entries as NOT-RUN so the summary still shows
    # every version that was in the plan.
    if [ "$_record_not_run_remaining" -eq 1 ]; then
        local remaining_idx
        for ((remaining_idx = plan_idx + 1; remaining_idx <= n; remaining_idx++)); do
            local _r_entry="${plan_lines[$remaining_idx]}"
            local _r_role _r_short _r_sha _r_branch _r_label _r_kind
            IFS='|' read -r _r_role _r_short _r_sha _r_branch _r_label <<< "$_r_entry"
            if [ -n "$_r_label" ]; then
                _r_kind="nightly"
            elif [ -n "$_r_branch" ]; then
                _r_kind="stable"
            else
                _r_kind="head"
            fi
            results+=("SKIP-NOT-RUN|${_r_sha:0:7}|$_r_sha|$_r_kind|$_r_branch|$_r_label")
        done
    fi

    # Build summary in three forms:
    #   - plain text table to stderr (always)
    #   - markdown table appended to $GITHUB_STEP_SUMMARY if set
    #   - JSON file at $out_dir/summary.json for tool/artifact consumption
    local summary_json="$out_dir/summary.json"
    local r status short_sha full_sha r_kind r_branch r_label glyph md_glyph display

    # Stderr plain table.
    {
        echo ""
        echo "=== cross-version-test summary ==="
        printf 'STATUS|KIND|SHORT|SHA|LABEL\n'
        for r in "${results[@]}"; do
            IFS='|' read -r status short_sha full_sha r_kind r_branch r_label <<< "$r"
            case "$status" in
                PASS)               glyph="✓ PASS" ;;
                FAIL)               glyph="✗ FAIL" ;;
                SKIP-BUILD)         glyph="⊘ SKIP (build failed)" ;;
                SKIP-NOT-RUN)       glyph="⊘ SKIP (not run — earlier required failure)" ;;
                SKIP-NO-HEALTHCHECK) glyph="⊘ SKIP (no healthcheck subcommand)" ;;
                *)                  glyph="? $status" ;;
            esac
            # Display: nightly tag label if present, else release branch
            # (stable rows), else empty (HEAD).
            display="${r_label:-$r_branch}"
            printf '%s|%s|%s|%s|%s\n' "$glyph" "$r_kind" "$short_sha" "$full_sha" "$display"
        done
    } | column -t -s '|' >&2

    # JSON summary.
    {
        printf '{\n  "completed_count": %s,\n' "$completed_count"
        printf '  "nightly_failure": %s,\n' "$nightly_failure"
        printf '  "required_failure": %s,\n' "$required_failure"
        printf '  "lenient_nightlies": %s,\n' "$lenient_nightlies"
        printf '  "results": [\n'
        local first=1
        for r in "${results[@]}"; do
            IFS='|' read -r status short_sha full_sha r_kind r_branch r_label <<< "$r"
            if [ "$first" -eq 0 ]; then printf ',\n'; fi
            first=0
            printf '    {"status":"%s","short":"%s","sha":"%s","kind":"%s","branch":"%s","label":"%s"}' \
                "$status" "$short_sha" "$full_sha" "$r_kind" "$r_branch" "$r_label"
        done
        printf '\n  ]\n}\n'
    } > "$summary_json"
    echo "::notice::summary JSON: $summary_json" >&2

    # Count libxmtp NDJSON log files produced this run for the summary.
    local log_count=0
    log_count=$(find "$out_dir" -maxdepth 2 -name '*.json' -type f \
                ! -name 'summary.json' 2>/dev/null | wc -l)

    # GitHub Actions step summary (markdown, renders in the Actions UI).
    if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
        {
            echo "## cross-version-test results"
            echo ""
            echo "| Status | Kind | Short | Label / Branch | SHA |"
            echo "|---|---|---|---|---|"
            for r in "${results[@]}"; do
                IFS='|' read -r status short_sha full_sha r_kind r_branch r_label <<< "$r"
                case "$status" in
                    PASS)               md_glyph="✅ PASS" ;;
                    FAIL)               md_glyph="❌ FAIL" ;;
                    SKIP-BUILD)         md_glyph="⊘ SKIP (build failed)" ;;
                    SKIP-NOT-RUN)       md_glyph="⊘ SKIP (not run — earlier required failure)" ;;
                    SKIP-NO-HEALTHCHECK) md_glyph="⊘ SKIP (no healthcheck subcommand)" ;;
                    *)                  md_glyph="❓ $status" ;;
                esac
                # Nightly rows carry a label (nightly.YYYYMMDD); stable rows
                # carry a branch (release/X); HEAD has neither.
                display="${r_label:-${r_branch:--}}"
                # shellcheck disable=SC2016  # backticks in template are markdown, not subshells
                printf '| %s | %s | `%s` | %s | `%s` |\n' \
                    "$md_glyph" "$r_kind" "$short_sha" "$display" "$full_sha"
            done
            echo ""
            echo "**completed_count=$completed_count required_failure=$required_failure nightly_failure=$nightly_failure lenient=$lenient_nightlies**"
            echo ""
            echo "**libxmtp NDJSON log files captured: ${log_count}** (download the run artifact to inspect)"
        } >> "$GITHUB_STEP_SUMMARY"
    fi

    # Cross-version compat requires at least two versions to successfully
    # complete `xdbg healthcheck` against the shared state, otherwise the
    # run did not actually exercise inter-version compatibility.
    if [ "$completed_count" -lt 2 ]; then
        echo "::error::run-sequence: insufficient cross-version coverage (completed=$completed_count); need at least 2 successful healthcheck runs" >&2
        exit 1
    fi

    # Exit-code policy:
    #   required failure → always non-zero (catches stable/HEAD regressions)
    #   nightly failure + lenient → exit 0 (warning already emitted)
    #   nightly failure + strict  → exit 1
    if [ "$required_failure" -eq 1 ]; then
        echo "::error::run-sequence: one or more required versions failed" >&2
        exit 1
    fi
    if [ "$nightly_failure" -eq 1 ] && [ "$lenient_nightlies" -eq 0 ]; then
        echo "::error::run-sequence: one or more nightlies failed (strict mode)" >&2
        exit 1
    fi
    if [ "$nightly_failure" -eq 1 ]; then
        echo "::notice::run-sequence completed with nightly failures (lenient mode; HEAD + required versions OK)" >&2
    else
        echo "::notice::run-sequence completed without failures" >&2
    fi
}

main "$@"
