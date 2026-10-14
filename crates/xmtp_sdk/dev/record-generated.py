#!/usr/bin/env python3
"""Record bindings with the same provenance schema as the CLI renderer."""

import argparse
import hashlib
import importlib.util
import json
from pathlib import Path


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def checkout_identity():
    """Read the SDK closure identities of this checkout through `sdk-artifacts.py`."""
    spec = importlib.util.spec_from_file_location(
        "artifacts", Path(__file__).with_name("sdk-artifacts.py")
    )
    artifacts = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(artifacts)
    return artifacts.source_hash(), artifacts.source_hash(True)


def record(
    generated,
    binaries,
    profile="release",
    features="",
    target="",
    source=None,
    generator=None,
):
    """Write each tree's receipt. Nix passes the evaluation-time identities."""
    trees = [
        tree
        for tree in generated.iterdir()
        if tree.is_dir() and tree.name != "runtimes"
    ]
    roles = {
        tree.name: "pure"
        if tree.name == "typescript-pure"
        else "wasm"
        if tree.name == "typescript-wasm"
        else "native"
        for tree in trees
    }
    for role in {"bindgen", *roles.values()}:
        if not binaries.get(role):
            raise ValueError(f"generated record requires {role} artifact")
    if source is None or generator is None:
        source, generator = checkout_identity()
    records = {
        role: {
            "source": source,
            "generator": generator,
            "features": features
            if role == "native"
            else "pure-only"
            if role == "pure"
            else "",
            "profile": profile,
            "target": target if role == "native" else "",
            "files": {Path(path).name: digest(path)},
        }
        for role, path in binaries.items()
        if path
    }
    contract = hashlib.sha256(
        json.dumps(
            {
                role: {
                    "files": {
                        name: checksum for name, checksum in item["files"].items()
                    },
                    "generator": generator,
                }
                for role, item in records.items()
            },
            sort_keys=True,
        ).encode()
    ).hexdigest()
    for tree in trees:
        role = roles[tree.name]
        files = {
            str(path.relative_to(tree)): digest(path)
            for path in sorted(tree.rglob("*"))
            if path.is_file()
            and path.name != "sdk-contract.json"
            and "node_modules" not in path.parts
        }
        (tree / "sdk-contract.json").write_text(
            json.dumps(
                {
                    "contract": contract,
                    "generator": generator,
                    "artifact": records[role],
                    "files": files,
                },
                indent=2,
            )
            + "\n"
        )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("generated", type=Path)
    for role in ("native", "bindgen", "wasm", "pure"):
        parser.add_argument("--" + role, type=Path, required=role == "bindgen")
    parser.add_argument("--profile", choices=("debug", "release"), default="release")
    parser.add_argument("--features", default="")
    parser.add_argument("--target", default="")
    # Nix passes both identities; other callers read them from the checkout.
    parser.add_argument("--source")
    parser.add_argument("--generator")
    args = parser.parse_args()
    record(
        args.generated,
        {role: getattr(args, role) for role in ("native", "bindgen", "wasm", "pure")},
        args.profile,
        args.features,
        args.target,
        args.source,
        args.generator,
    )


if __name__ == "__main__":
    main()
