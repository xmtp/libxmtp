"""Record local RN root identities and bounded realization markers without building."""

import json
import os
from pathlib import Path
import subprocess
import sys


def output(*command):
    return subprocess.check_output(command, text=True).strip()


stage, system, version, mode, phase = sys.argv[1:]
path = Path(f"target/rn-evidence/{stage}-{phase}.json")
if phase == "before":
    # Versioned bundles and their members must use the release package too.
    expression = """
      let f = builtins.getFlake ("git+file://" + toString ./.);
          system = builtins.getEnv "RN_SYSTEM";
          stage = builtins.getEnv "RN_STAGE";
          members = f.ciStages.${system}.${stage}.passthru.members;
          version = builtins.getEnv "RN_PACKAGE_VERSION";
          consumers = f.legacyPackages.${system}.reactNative.mkConsumers {
            package = f.legacyPackages.x86_64-linux.reactNative.mkPackage { inherit version; };
          };
          roots = if stage == "components" then members // f.ciStages.${system}.native-base.passthru.members
                  else if version == "" then members
                  else consumers.passthru.members // { bundles = consumers; }
                    // builtins.intersectAttrs { react-native-consumer-template-bare = null; react-native-consumer-template-expo = null; } members;
      in builtins.mapAttrs (_: drv: { inherit (drv) drvPath outPath; }) roots
    """
    roots = json.loads(
        output(
            "nix",
            "eval",
            "--option",
            "allow-import-from-derivation",
            "false",
            "--json",
            "--impure",
            "--expr",
            expression,
        )
    )
    receipt = {
        "sourceSha": output("git", "rev-parse", "HEAD"),
        "system": system,
        "stage": stage,
        "version": version,
        "cacheMode": mode,
        "runId": os.environ.get("GITHUB_RUN_ID"),
        "roots": roots,
    }
    if system == "aarch64-darwin":
        receipt["xcode"] = {
            "globalDeveloperDir": output("xcode-select", "-p"),
            "version": output("xcodebuild", "-version"),
            "declaredNative": json.loads(
                output("nix", "eval", "--json", ".#lib.xcode")
            ),
            "declaredRN": json.loads(
                output("nix", "eval", "--json", ".#lib.reactNativeXcode")
            ),
        }
else:
    receipt = json.loads(path.with_name(f"{stage}-before.json").read_text())
    receipt["buildSeconds"] = (
        int(os.environ["RN_BUILD_SECONDS"])
        if os.environ.get("RN_BUILD_SECONDS")
        else None
    )
    log = Path(os.environ["RUNNER_TEMP"], "rn-build.log")
    receipt["realizationMarkers"] = (
        [
            line
            for line in log.read_text().splitlines()
            if "building '" in line or "copying path '" in line
        ][-200:]
        if log.exists()
        else []
    )
for root in receipt["roots"].values():
    valid = subprocess.run(
        ["nix-store", "--check-validity", root["outPath"]], capture_output=True
    ).returncode
    root["validityExitCode"] = valid
    if not valid:
        info = json.loads(output("nix", "path-info", "--json", root["outPath"]))
        root["narHash"] = (
            info[0] if isinstance(info, list) else info[root["outPath"]]
        )["narHash"]
    else:
        root.pop("narHash", None)
path.parent.mkdir(parents=True, exist_ok=True)
path.write_text(json.dumps(receipt | {"phase": phase}, indent=2, sort_keys=True) + "\n")
