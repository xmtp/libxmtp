"""Adapt only the locked installed Expo SwiftPM phase inside a Nix app build."""

import hashlib
import json
import os
from pathlib import Path
import stat
import tempfile


ORIGINAL_SHA256 = "de9165801ca1598a24bf8fc83a456fef8321daaa5dc3d523c0146086b6b4ddb3"
REPLACEMENTS = (
    (
        '  (cd "$PACKAGE_DIR" && env -i PATH="$PATH" HOME="$HOME" PODS_ROOT="$PODS_ROOT" RN_ROOT="$RN_ROOT" \\\n',
        '  mkdir -p "${DERIVED_DATA_PATH}/NixHome" "${DERIVED_DATA_PATH}/ManifestModuleCache"\n'
        '  (cd "$PACKAGE_DIR" && env -i PATH="$PATH" HOME="${DERIVED_DATA_PATH}/NixHome" PODS_ROOT="$PODS_ROOT" RN_ROOT="$RN_ROOT" \\\n'
        '    CFFIXED_USER_HOME="${DERIVED_DATA_PATH}/NixHome" SWIFTPM_MODULECACHE_OVERRIDE="${DERIVED_DATA_PATH}/ManifestModuleCache" \\\n',
    ),
    (
        "    xcodebuild \\\n    build \\\n",
        "    xcodebuild \\\n    -IDEPackageSupportDisableManifestSandbox=YES \\\n    build \\\n",
    ),
    (
        "    SKIP_INSTALL=NO \\\n",
        "    SKIP_INSTALL=NO \\\n    CODE_SIGNING_ALLOWED=NO \\\n",
    ),
)


MACRO_SHA256 = "e4f446f9ba80e4316b20ac077da2fb19631ce5e2b94f9f172d38fbd623d20d9c"
MACRO_FLAG = " -Xfrontend -disable-sandbox"


def macro_config_plan(work: Path) -> tuple[list[tuple[Path, str]], dict]:
    package = (work / "node_modules/expo-modules-macros").resolve(strict=True)
    if not package.is_relative_to(work):
        raise ValueError("Expo macro package escapes the owned work directory")
    metadata = json.loads((package / "package.json").read_text())
    if (
        metadata.get("name") != "expo-modules-macros"
        or metadata.get("version") != "0.20.0"
    ):
        raise ValueError("Unexpected Expo macro package identity")
    executable = (package / "apple/ExpoModulesMacros").resolve(strict=True)
    if not executable.is_relative_to(package) or not executable.is_file():
        raise ValueError("Expo macro executable escapes its admitted package")
    if hashlib.sha256(executable.read_bytes()).hexdigest() != MACRO_SHA256:
        raise ValueError("Unexpected locked Expo macro executable hash")
    root = (work / "ios/Pods/Target Support Files").resolve(strict=True)
    if not root.is_relative_to(work) or root.stat().st_uid != os.getuid():
        raise ValueError("Generated Expo configurations are not build-owned")
    token = f'-load-plugin-executable "{executable}#ExpoModulesMacros"'
    changes = []
    matches = 0
    for candidate in sorted(root.rglob("*.xcconfig")):
        path = candidate.resolve(strict=True)
        if not path.is_relative_to(root) or path.stat().st_uid != os.getuid():
            raise ValueError("Generated Expo configuration escapes its owned tree")
        source = path.read_bytes().decode("utf-8")
        lines = []
        for line in source.splitlines(keepends=True):
            if line.startswith("OTHER_SWIFT_FLAGS = ") and "ExpoModulesMacros" in line:
                if "\r" in line:
                    raise ValueError(
                        "Unexpected generated Expo configuration line ending"
                    )
                if line.count(token) != 1:
                    raise ValueError("Unexpected generated Expo macro load flag")
                if "-disable-sandbox" in line:
                    if line.count(MACRO_FLAG) != 1:
                        raise ValueError("Unexpected generated macro sandbox flag")
                else:
                    ending = "\n" if line.endswith("\n") else ""
                    line = line.removesuffix("\n") + MACRO_FLAG + ending
                matches += 1
            lines.append(line)
        result = "".join(lines)
        if result != source:
            changes.append((path, result))
    if not matches:
        raise ValueError("No generated configurations load the locked Expo macro")
    return changes, {
        "package": "expo-modules-macros@0.20.0",
        "executableSha256": MACRO_SHA256,
        "configuredFlagLines": matches,
    }


def replace_owned_file(path: Path, content: str) -> None:
    mode = stat.S_IMODE(path.stat().st_mode)
    fd, temporary = tempfile.mkstemp(prefix=".nix-expo-", dir=path.parent)
    try:
        with os.fdopen(fd, "w") as output:
            output.write(content)
        os.chmod(temporary, mode)
        os.replace(temporary, path)
    finally:
        Path(temporary).unlink(missing_ok=True)


def adapt(work: Path, build_root: Path) -> dict:
    work = work.resolve(strict=True)
    build_root = build_root.resolve(strict=True)
    if not work.is_relative_to(build_root) or work == build_root:
        raise ValueError("Expo adaptation requires an owned Nix work directory")
    if work.stat().st_uid != os.getuid():
        raise ValueError("Expo work directory is not owned by the build user")
    package = (work / "node_modules/expo-modules-jsi").resolve(strict=True)
    if not package.is_relative_to(work):
        raise ValueError("Installed Expo package escapes the owned work directory")
    metadata = json.loads((package / "package.json").read_text())
    if (
        metadata.get("name") != "expo-modules-jsi"
        or metadata.get("version") != "58.0.15"
    ):
        raise ValueError("Unexpected ExpoModulesJSI package identity")
    script = (package / "apple/scripts/build-xcframework.sh").resolve(strict=True)
    if not script.is_relative_to(package) or not script.is_file():
        raise ValueError("Expo phase script escapes the admitted package")
    source = script.read_bytes().decode("utf-8")
    original = source
    if hashlib.sha256(source.encode()).hexdigest() != ORIGINAL_SHA256:
        for before, after in reversed(REPLACEMENTS):
            if original.count(after) != 1:
                raise ValueError("Unexpected locked Expo phase script bytes")
            original = original.replace(after, before, 1)
        if hashlib.sha256(original.encode()).hexdigest() != ORIGINAL_SHA256:
            raise ValueError("Unexpected locked Expo phase script hash")
    result = original
    for before, after in REPLACEMENTS:
        if result.count(before) != 1:
            raise ValueError("Locked Expo phase anchor is ambiguous")
        result = result.replace(before, after, 1)
    configurations, macro_identity = macro_config_plan(work)
    # Validate the complete plan before replacing any installed or generated file.
    if result != source:
        replace_owned_file(script, result)
    for configuration, content in configurations:
        replace_owned_file(configuration, content)
    return {
        "package": "expo-modules-jsi@58.0.15",
        "originalSha256": ORIGINAL_SHA256,
        "adaptedSha256": hashlib.sha256(result.encode()).hexdigest(),
        "macro": macro_identity,
    }


if __name__ == "__main__":
    if not os.environ.get("NIX_BUILD_TOP"):
        raise RuntimeError("Expo adaptation is only for the Nix consumer builder")
    if Path.cwd().name != "ios":
        raise RuntimeError("Expo CocoaPods adapter requires the app ios directory")
    print(json.dumps(adapt(Path.cwd().parent, Path(os.environ["NIX_BUILD_TOP"]))))
