#!/usr/bin/env python3
"""Install independent codec author fixtures beside copied SDK products."""

import json
import os
from pathlib import Path
import shutil

root = Path(__file__).resolve().parents[3]
generated = Path(os.environ["XMTP_SDK_GENERATED_DIR"]).resolve()
packages = os.environ.get("XMTP_SDK_PACKAGES_DIR")
fixture = root / "crates/xmtp_sdk/conformance/public/codec-author"
stage = root / "target/sdk-codec-author"
shutil.rmtree(stage, ignore_errors=True)

for target, sdk_name in [("node", "xmtp-sdk"), ("browser", "xmtp-sdk-browser")]:
    consumer = stage / target
    modules = consumer / "node_modules"
    sdk = modules / sdk_name
    if packages:
        # Preserve the release product's manifest, declarations and assets.
        shutil.copytree(
            Path(packages).resolve() / target,
            sdk,
        )
    else:
        parts = (
            ["typescript-napi"]
            if target == "node"
            else ["typescript-wasm", "typescript-pure"]
        )
        for part in parts:
            destination = sdk if target == "node" else sdk / part
            shutil.copytree(
                generated / part,
                destination,
                ignore=shutil.ignore_patterns("node_modules"),
            )
        manifest = {"name": sdk_name, "private": True, "type": "module"}
        if target == "node":
            manifest.update(
                exports={".": "./index.ts"}, imports={"#xmtp/binding": "./xmtp_sdk.ts"}
            )
        else:
            manifest["exports"] = {
                ".": "./typescript-wasm/index.ts",
                "./pure": "./typescript-pure/index.ts",
            }
        (sdk / "package.json").write_text(json.dumps(manifest, indent=2) + "\n")
    for name in [] if packages else ["core", "node" if target == "node" else "wasm"]:
        link = sdk / "node_modules/@ubjs" / name
        link.parent.mkdir(parents=True, exist_ok=True)
        link.symlink_to(os.environ[f"SDK_AUTHOR_{name.upper()}"])
    author = modules / "@example/reading-codec"
    shutil.copytree(fixture, author)
    # Each target uses its supported package root. No binding path is substituted.
    for source in author.rglob("*.ts"):
        source.write_text(
            source.read_text().replace('"xmtp-sdk"', json.dumps(sdk_name))
        )
    author_manifest = json.loads((author / "package.json").read_text())
    author_manifest["dependencies"] = {sdk_name: f"file:../../{sdk_name}"}
    (author / "package.json").write_text(json.dumps(author_manifest, indent=2) + "\n")
    for name in ["positive.ts", "negative.ts"]:
        shutil.move(author / name, consumer / name)
    (consumer / "package.json").write_text('{"private":true,"type":"module"}\n')
    (consumer / "entry.ts").write_text(
        'export { exercise } from "@example/reading-codec/proof";\n'
        + f'export type {{ Signer }} from "{sdk_name}";\n'
    )
print("Installed independent Node and browser codec author packages")
