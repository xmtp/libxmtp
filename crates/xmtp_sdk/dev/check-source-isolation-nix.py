#!/usr/bin/env python3
"""Check SDK derivation changes through isolated source mutations."""

import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[3]
EXPRESSION = r"""
systems:
let
  p = systems.${builtins.currentSystem};
  compiler = p.xmtp-sdk-libs.compilation;
in {
  native = compiler.drvPath;
  deps = compiler.cargoArtifacts.drvPath;
  bindgen = p.xmtp-sdk-bindgen.drvPath;
  bindgenDeps = p.xmtp-sdk-bindgen.cargoArtifacts.drvPath;
  rendering = p.xmtp-sdk-generated.rendering.drvPath;
  product = p.xmtp-sdk-generated.drvPath;
  nativeProduct = p.xmtp-sdk-libs.drvPath;
  provenance = toString p.xmtp-sdk-generated.src;
  swift = if builtins.hasAttr "ios-libs" p
    then let bindings = p.ios-libs.swiftBindings;
      in (bindings.rendering or bindings).drvPath else null;
}
"""


def identity(root):
    spec = importlib.util.spec_from_file_location(
        "artifacts", root / "crates/xmtp_sdk/dev/sdk-artifacts.py"
    )
    artifacts = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(artifacts)
    return {"native": artifacts.source_hash(), "generator": artifacts.source_hash(True)}


def evaluate(root):
    result = json.loads(
        subprocess.check_output(
            ["nix", "eval", "--impure", "--json", ".#packages", "--apply", EXPRESSION],
            cwd=root,
        )
    )
    result["identity"] = identity(root)
    actual = identity(Path(result["provenance"]))
    if result["identity"] != actual:
        raise ValueError(
            f"{root.name}: provenance source identity differs from snapshot"
        )
    return result


def atomic_append(path, suffix):
    replacement = path.with_name(path.name + ".source-control")
    replacement.write_bytes(path.read_bytes() + suffix.encode())
    replacement.replace(path)


def main():
    stable_compilers = ("native", "deps", "bindgen", "bindgenDeps")
    stable_all = (*stable_compilers, "rendering", "swift")
    cases = [
        (
            "application-rust",
            "apps/backend/src/lib.rs",
            "\n// source control\n",
            stable_all,
            ("product", "nativeProduct"),
            "native",
        ),
        (
            "sdk-test",
            "crates/xmtp_sdk/dev/test-sdk-artifacts.py",
            "\n# source control\n",
            stable_all,
            ("product", "nativeProduct"),
            None,
        ),
        (
            "runtime",
            "apps/xmtp_sdk_bindgen/runtime/swift/SDKClient.swift",
            "\n// source control\n",
            stable_compilers,
            ("rendering", "swift", "product", "nativeProduct"),
            "generator",
        ),
        (
            "sdk-rust",
            "crates/xmtp_sdk/src/lib.rs",
            "\n// source control\n",
            ("deps", "bindgen", "bindgenDeps"),
            ("native", "rendering", "swift", "product"),
            "native",
        ),
        (
            "schema",
            "proto/message_contents/signature.proto",
            "\n// source control\n",
            (),
            ("native", "deps", "rendering", "swift", "product"),
            "native",
        ),
        (
            "migration",
            "crates/xmtp_db/migrations/2026-09-24-000000_attachments/up.sql",
            "\n-- source control\n",
            ("deps", "bindgen", "bindgenDeps"),
            ("native", "rendering", "swift", "product"),
            "native",
        ),
        (
            "address-registry",
            "crates/xmtp_attachments/src/address-registry.txt",
            "\n# source control\n",
            ("deps", "bindgen", "bindgenDeps"),
            ("native", "rendering", "swift", "product"),
            "native",
        ),
        (
            "chain-urls",
            "crates/xmtp_id/src/scw_verifier/chain_urls_default.json",
            "\n ",
            ("deps", "bindgen", "bindgenDeps"),
            ("native", "rendering", "swift", "product"),
            "native",
        ),
        (
            "signature-bytecode",
            "crates/xmtp_id/src/scw_verifier/signature_validation.hex",
            "\n",
            ("deps", "bindgen", "bindgenDeps"),
            ("native", "rendering", "swift", "product"),
            "native",
        ),
        (
            "template",
            "apps/xmtp_sdk_bindgen/templates/bridge/logging.ts",
            "\n// source control\n",
            ("native", "deps", "bindgenDeps"),
            ("bindgen", "rendering", "swift", "product"),
            "generator",
        ),
        (
            "configuration",
            "apps/xmtp_sdk_bindgen/uniffi-global.toml",
            "\n# source control\n",
            ("native", "deps", "bindgenDeps"),
            ("bindgen", "rendering", "swift", "product"),
            "generator",
        ),
    ]
    with tempfile.TemporaryDirectory(prefix="xmtp-sdk-source-nix-") as folder:
        folder = Path(folder)
        baseline_root = folder / "baseline"
        baseline_root.mkdir()
        tracked = (
            subprocess.check_output(["git", "ls-files", "-z"], cwd=ROOT)
            .decode()
            .split("\0")
        )
        for relative in tracked:
            if not relative:
                continue
            source = ROOT / relative
            target = baseline_root / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            if source.is_symlink():
                target.symlink_to(source.readlink())
            else:
                shutil.copy2(source, target)
        baseline = evaluate(baseline_root)
        print(json.dumps({"baseline": baseline}, sort_keys=True), flush=True)
        for name, relative, suffix, unchanged, changed, identity_kind in cases:
            snapshot = folder / name
            shutil.copytree(
                baseline_root, snapshot, copy_function=os.link, symlinks=True
            )
            atomic_append(snapshot / relative, suffix)
            actual = evaluate(snapshot)
            for key in unchanged:
                if actual[key] != baseline[key]:
                    raise ValueError(
                        f"{name}: {key} changed for an unrelated compiler input"
                    )
            for key in changed:
                if baseline[key] is not None and actual[key] == baseline[key]:
                    raise ValueError(
                        f"{name}: {key} did not change for a required input"
                    )
            for kind in ("native", "generator"):
                expected_change = kind == identity_kind or (
                    kind == "native"
                    and identity_kind == "generator"
                    and relative.endswith(".rs")
                )
                if (
                    actual["identity"][kind] != baseline["identity"][kind]
                ) != expected_change:
                    raise ValueError(f"{name}: unexpected {kind} source identity")
            print(
                json.dumps(
                    {
                        "case": name,
                        "unchanged": unchanged,
                        "changed": changed,
                        "result": actual,
                    },
                    sort_keys=True,
                ),
                flush=True,
            )


if __name__ == "__main__":
    main()
