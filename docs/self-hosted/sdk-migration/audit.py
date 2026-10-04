#!/usr/bin/env python3
"""Generate Phase 2 file ownership from tracked paths and the existing inventory."""

from collections import Counter
from hashlib import sha256
from pathlib import Path
import re
import fnmatch
import ast
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[3]
PLATFORM_PREFIXES = {
    "iOS": ("sdks/ios/", "crates/xmtp_sdk/conformance/swift/"),
    "Android": (
        "sdks/android/",
        "apps/android/xmtpv3_example/",
        "crates/xmtp_sdk/conformance/kotlin/",
    ),
    "Node plus agent": (
        "sdks/node/",
        "sdks/agent/",
        "apps/cli/",
        "crates/xmtp_sdk/conformance/ts/",
    ),
    "Browser": (
        "sdks/browser/",
        "apps/web-chat/",
        "crates/xmtp_sdk/conformance/browser/",
    ),
}
PLATFORM_FILES = {
    "iOS": {"Package.swift", "Package.resolved", "nix/package/ios.nix"},
    "Android": {"nix/android-packages.nix"},
    "Node plus agent": {"nix/package/node.nix"},
    "Browser": {"nix/package/wasm.nix", "nix/package/wasm-nextest.nix"},
}
WORKFLOWS = {
    "iOS": {"lint-ios", "release-ios", "test-ios"},
    "Android": {"lint-android", "release-android", "test-android"},
    "Node plus agent": {
        "lint-node",
        "release-agent-sdk",
        "release-cli",
        "release-node-sdk",
        "test-agent-sdk",
        "test-node-sdk",
    },
    "Browser": {"deploy-web-chat", "release-browser-sdk", "test-browser-sdk"},
}


def owner(path):
    candidates = []
    for platform, prefixes in PLATFORM_PREFIXES.items():
        if path.startswith(prefixes) or path in PLATFORM_FILES[platform]:
            candidates.append(platform)
        if (
            path.startswith(".github/workflows/")
            and Path(path).stem in WORKFLOWS[platform]
        ):
            candidates.append(platform)
    if path.startswith("apps/docs/examples/"):
        stem = Path(path).stem
        if stem.endswith("-browser"):
            candidates.append("Browser")
        elif stem.endswith("-node") or stem.startswith("agents-"):
            candidates.append("Node plus agent")
    assert len(set(candidates)) <= 1, f"multiple owners: {path}: {candidates}"
    return candidates[0] if candidates else "Integration writer"


def approved_binding_names():
    source = subprocess.check_output(
        ["git", "show", "b94716408:dev/sdk/manifest_rules.py"], cwd=ROOT
    ).decode()
    for node in ast.parse(source).body:
        if (
            isinstance(node, ast.AnnAssign)
            and isinstance(node.target, ast.Name)
            and node.target.id == "BINDING_REEXPORT_REMOVALS"
        ):
            return set(ast.literal_eval(node.value))
    raise ValueError("approved #4327 removal table is missing")


def approved_removal(sdk, symbol, binding_names):
    if sdk in {"Node", "Browser"}:
        return symbol in binding_names
    return (
        symbol == "ReactionCodec"
        or symbol.startswith("ReactionCodec.")
        or (
            sdk == "Swift"
            and symbol
            in {
                "ContentCodec.==",
                "ContentCodec.hash",
                "ContentCodec.id",
                "ContentCodec.description",
            }
        )
    )


def main():
    output = Path(sys.argv[1])
    output.mkdir(parents=True, exist_ok=True)
    tracked = (
        subprocess.check_output(["git", "ls-files", "-z"], cwd=ROOT)
        .decode()
        .split("\0")
    )
    selected = set()
    for path in filter(None, tracked):
        if path.startswith(
            (
                "sdks/",
                "bindings/",
                "apps/docs/",
                ".github/workflows/",
                "examples/",
                "apps/cli/",
                "apps/web-chat/",
                "apps/android/",
                "crates/xmtp_sdk/conformance/",
                "dev/sdk/",
                "dev/release-tools/",
                "dev/updatecli/",
                "nix/",
            )
        ) or path in {
            "Cargo.toml",
            "Cargo.lock",
            "flake.nix",
            "flake.lock",
            "package.json",
            "pnpm-lock.yaml",
            "pnpm-workspace.yaml",
            "justfile",
            "Package.swift",
            "Package.resolved",
            ".oxfmtrc.json",
        }:
            selected.add(path)
            continue
        source = ROOT / path
        if source.is_file():
            data = source.read_bytes()
            if b"\0" not in data and re.search(
                rb"@xmtp/(?:node|browser|agent|content-type|content-types|proto|wasm|bindings)|bindings/(?:mobile|node|wasm)|sdks/(?:ios|android|node|browser|agent)",
                data,
            ):
                selected.add(path)
    # Include new guide files before their first commit.
    selected.update(
        p.relative_to(ROOT).as_posix()
        for p in (ROOT / "docs/self-hosted/sdk-migration").glob("*")
        if p.is_file()
    )
    files = [(path, owner(path)) for path in sorted(selected)]
    (output / "paths.tsv").write_text(
        "Path\tOwner\n" + "".join(f"{p}\t{o}\n" for p, o in files)
    )
    manifest = ROOT / "docs/self-hosted/sdk-api-manifest.md"
    binding_names = approved_binding_names()
    sdk = None
    rows = []
    open_items = []
    for line_no, line in enumerate(manifest.read_text().splitlines(), 1):
        if line in {"## Swift", "## Kotlin", "## Node", "## Browser"}:
            sdk = line.removeprefix("## ")
        elif line.startswith("## "):
            sdk = None
        if sdk and line.startswith("| `"):
            cells = re.split(r"(?<!\\)\|", line)[1:-1]
            status = cells[3].strip()
            assert status in {
                "generated",
                "static runtime",
                "platform helper",
                "approved removal",
                "proposed removal",
            }, (line_no, status)
            source = re.search(r"Source: `([^`]+)`", cells[-1])
            assert source, f"manifest row has no source: {line_no}"
            source_path = {
                "@xmtp/node-bindings": "sdks/node/src/index.ts",
                "@xmtp/wasm-bindings": "sdks/browser/src/index.ts",
            }.get(source[1], source[1])
            matches = [p for p in selected if fnmatch.fnmatchcase(p, source_path)]
            assert matches, f"manifest source has no owner: {line_no}: {source_path}"
            assert len({owner(p) for p in matches}) == 1, (
                f"manifest family has multiple owners: {line_no}"
            )
            symbol = re.search(r"`([^`]+)`", cells[0])[1]
            accepted = status == "proposed removal" and approved_removal(
                sdk, symbol, binding_names
            )
            disposition = (
                "approved removal"
                if status == "approved removal" or accepted
                else "new owner decision"
                if status == "proposed removal"
                else "final public member proof pending"
            )
            provenance = (
                "Owner decisions item 8; #4327 b94716408"
                if accepted
                else "existing manifest"
            )
            rows.append(
                (line_no, sdk, status, owner(source_path), disposition, provenance)
            )
        if re.match(r"^- (Swift|Kotlin|Node|Browser) `", line):
            open_items.append(line_no)
    assert rows, "manifest audit found no rows"
    (output / "manifest-rows.tsv").write_text(
        "Manifest line\tSDK\tStatus\tOwner\tDisposition\tProvenance\n"
        + "".join("\t".join(map(str, r)) + "\n" for r in rows)
    )
    map_path = ROOT / "dev/sdk/binding-test-map.tsv"
    tests = [
        r.split("\t")
        for r in map_path.read_text().splitlines()
        if r and not r.startswith("#")
    ]
    assert all(len(r) == 3 for r in tests), "invalid existing mobile test map"
    assert len({(r[0], r[1]) for r in tests}) == len(tests), (
        "duplicate existing mobile test"
    )
    (output / "mobile-tests.tsv").write_text(
        "Map row\tOwner\tHost coverage audit\tDeletion writer\n"
        + "".join(
            f"{n}\tIntegration writer\tiOS and Android retained-test audit\tIntegration writer in the second mobile switch\n"
            for n in range(2, len(tests) + 2)
        )
    )
    summary = {
        "commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT)
        .decode()
        .strip(),
        "files": dict(Counter(o for _, o in files)),
        "manifest_rows": len(rows),
        "statuses": dict(Counter(r[2] for r in rows)),
        "open_manifest_entries": len(open_items),
        "owner_approved_proposed_rows": sum(
            r[2] == "proposed removal" and r[4] == "approved removal" for r in rows
        ),
        "unapproved_proposed_rows": sum(r[4] == "new owner decision" for r in rows),
        "mobile_test_rows": len(tests),
        "manifest_sha256": sha256(manifest.read_bytes()).hexdigest(),
        "test_map_sha256": sha256(map_path.read_bytes()).hexdigest(),
    }
    import json

    (output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
