#!/usr/bin/env python3
"""Bind reference caches to current source, tools, runner, and output bytes."""

import argparse
import ast
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import re
import shutil
import shlex
import subprocess
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[2]
RECEIPT = ".ci-reference.json"
SCHEMA = 3
POLICY_PATH = "dev/ci/docs-reference-cache.py"
CONFIG_READER = "crates/xmtp_sdk/dev/sdk-build-inputs.py"
spec = importlib.util.spec_from_file_location(
    "reference_cargo_inputs", ROOT / CONFIG_READER
)
cargo_inputs = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cargo_inputs)

# These scripts read compiler inputs or generated OUT_DIR files, not prose.
# Any change or new build script selects the complete source tree instead.
REVIEWED_BUILD_SCRIPTS = {
    "crates/xmtp_macro/build.rs": "cfb0fe953a1e427a183e87ced9d716c715876ac6b62310f43cbfb5775a6358d4",
    "crates/xmtp-workspace-hack/build.rs": "13f6c3ba6785b4fee0cf3bd6cdd57b9b4875475589613609b4b9c5e3d7838b26",
    "apps/xmtp_debug/build.rs": "91ddce44649dd211b1ca90fafde5082ddbe259d6941fdfe4a6b9684d96768618",
    "crates/xmtp_proto/build.rs": "96acfc773639a04771fe70f01d813047158465f74394a25d0018ce4f0427982d",
}
REVIEWED_DYNAMIC_INCLUDES = {
    "crates/xmtp_proto/src/lib.rs": "a782d7f4cfc063f1de95fc65ce53451cc1a0072235490070a8c124ad36899bda",
    "crates/xmtp_macro/src/facade_markers_test.rs": "8dfcabb95d5e7e09caa6bd6cb160787643cc1b030b233c174cc1a0a9fbffdcbb",
}
INCLUDE = re.compile(r'include_(?:str|bytes)\s*!\s*\(\s*"([^"\\]*)"\s*\)')
INCLUDE_NAME = re.compile(r"\binclude_(?:str|bytes)\b")
# Review compiler readers before updating these digests. A changed reader can
# add ignored or external inputs that a complete Git tree does not cover.
REVIEWED_READERS = {
    "rust": "d47a7586eb17105301b3274f1af68a8aca72306bc205d3a2cdebc8c10507d5a1",
    "kotlin": "83e7d12c47da9f21ded5e2472b43bb4ecc668803908d530c906660d979f47087",
    "swift": "9d36c20a70b6e78019e1157adf8ca84300ab855af2301fe02c8b661eea45d14c",
}

JS_FAMILIES = ("sdks/node/", "sdks/browser/", "sdks/agent/")
JS_SOURCES = (".ts", ".tsx", ".mts", ".cts", ".js", ".jsx", ".mjs", ".cjs")


def encoded(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":")).encode()


def digest(data):
    return hashlib.sha256(data).hexdigest()


def file_record(path, root):
    if path.is_symlink():
        target = path.resolve()
        if not target.is_relative_to(root.resolve()):
            raise ValueError(f"Source link escapes the repository: {path}")
        if target.is_file():
            content = digest(target.read_bytes())
        elif target.is_dir():
            content = inventory(target, skip_receipt=False)
        else:
            raise ValueError(f"Source link target is missing: {path}")
        return {"symlink": os.readlink(path), "content": content}
    if not path.exists():
        return {"deleted": True}
    if not path.is_file():
        raise ValueError(f"Source input is not a file: {path}")
    return {"sha256": digest(path.read_bytes())}


def source_names(root):
    names = (
        subprocess.check_output(
            ["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard"],
            cwd=root,
        )
        .decode()
        .split("\0")
    )
    names = sorted(set(filter(None, names)))
    if not names:
        raise ValueError("Reference source inputs are missing")
    return names


def normalized_policy(data):
    source = data.decode()
    nodes = [
        node
        for node in ast.parse(source).body
        if isinstance(node, ast.Assign)
        and len(node.targets) == 1
        and isinstance(node.targets[0], ast.Name)
        and node.targets[0].id == "REVIEWED_READERS"
    ]
    if len(nodes) != 1 or not isinstance(nodes[0].value, ast.Dict):
        raise ValueError("Reviewed readers must have one literal dictionary")
    node = nodes[0]
    values = node.value
    if (
        len(values.keys) != 3
        or any(not isinstance(key, ast.Constant) for key in values.keys)
        or {key.value for key in values.keys} != {"rust", "kotlin", "swift"}
        or any(
            not isinstance(value, ast.Constant) or not isinstance(value.value, str)
            for value in values.values
        )
    ):
        raise ValueError("Reviewed readers must contain only the three literal pins")
    lines = data.splitlines(keepends=True)
    spans = [
        (
            sum(map(len, lines[: value.lineno - 1])) + value.col_offset,
            sum(map(len, lines[: value.end_lineno - 1])) + value.end_col_offset,
        )
        for value in values.values
    ]
    for start, end in sorted(spans, reverse=True):
        data = data[:start] + b"'<reviewed-value>'" + data[end:]
    return data


def reader_contract(root, names, kind):
    selected = set()
    macros = []
    for name in names:
        path = root / name
        if path.name == "Cargo.toml" and path.is_file():
            manifest = tomllib.loads(path.read_text())
            selected.add(name)
            if manifest.get("lib", {}).get("proc-macro"):
                macros.append(path.parent.relative_to(root).as_posix() + "/")
        if (
            name.endswith(".nix")
            or name.startswith((".cargo/", ".github/"))
            or path.name == "build.rs"
            or name
            in (
                "Cargo.lock",
                "flake.lock",
                "rust-toolchain.toml",
                "justfile",
                "dev/nix-shell",
                "dev/kache-env",
                "dev/kache-darwin-wrapper",
                "dev/agent-run",
                "dev/gen-error-glossary",
                POLICY_PATH,
            )
        ):
            selected.add(name)
    readers = (*macros, "apps/error_glossary/", "apps/xmtp_sdk_bindgen/")
    for name in names:
        if name.startswith(readers):
            selected.add(name)
        if kind == "swift" and (
            name.startswith(("sdks/ios/dev/", "sdks/ios/script/"))
            or name in ("Package.swift", "Package.resolved", "sdks/ios/ios.just")
        ):
            selected.add(name)
        if kind == "kotlin" and (
            name.startswith(("sdks/android/dev/", "sdks/android/gradle/"))
            or name.endswith(".gradle")
            and name.startswith("sdks/android/")
            or name in ("sdks/android/android.just", "sdks/android/gradlew")
        ):
            selected.add(name)
        if (
            name == CONFIG_READER
            or kind != "rust"
            and name
            in (
                "crates/xmtp_sdk/dev/record-generated.py",
                "crates/xmtp_sdk/dev/sdk-artifacts.py",
            )
        ):
            selected.add(name)
    records = {name: file_record(root / name, root) for name in sorted(selected)}
    if POLICY_PATH not in records:
        raise ValueError("Reference policy source is missing")
    records[POLICY_PATH] = {
        "sha256": digest(normalized_policy((root / POLICY_PATH).read_bytes()))
    }
    return digest(encoded(records))


def unrelated_source(name, kind):
    if name == "docs/error_glossary.md":
        return False
    if name.startswith("docs/") and name.endswith(".md"):
        return True
    if name.startswith("apps/docs/src/content/docs/") and name.endswith(
        (".md", ".mdx")
    ):
        return True
    if name.startswith(JS_FAMILIES) and name.endswith(JS_SOURCES):
        return True
    if kind != "swift" and name.startswith("sdks/ios/") and name.endswith(".swift"):
        return True
    if (
        kind != "kotlin"
        and name.startswith("sdks/android/")
        and name.endswith((".kt", ".java"))
    ):
        return True
    return False


def source_snapshot(root, kind="rust"):
    names = source_names(root)
    readers = reader_contract(root, names, kind)
    known_readers = readers == REVIEWED_READERS[kind]
    selected = {name for name in names if not unrelated_source(name, kind)}
    complete_tree = not known_readers
    ignored = (
        subprocess.check_output(
            [
                "git",
                "ls-files",
                "-z",
                "--others",
                "--ignored",
                "--exclude-standard",
                "--",
                "crates",
                "bindings",
            ],
            cwd=root,
        )
        .decode()
        .split("\0")
    )
    glossary_inputs = {
        name
        for name in ignored
        if name.endswith(".rs") or Path(name).name == "Cargo.toml"
    }
    for name in names:
        path = root / name
        if path.name == "build.rs" and path.is_file():
            if digest(path.read_bytes()) != REVIEWED_BUILD_SCRIPTS.get(name):
                complete_tree = True
        if path.suffix != ".rs" or not path.is_file():
            continue
        source = path.read_text()
        includes = list(INCLUDE.finditer(source))
        for match in includes:
            included = (path.parent / match[1]).resolve()
            try:
                relative = included.relative_to(root.resolve()).as_posix()
            except ValueError:
                raise ValueError(
                    f"Embedded source escapes the repository: {name}"
                ) from None
            # An include can read any language, ignored file, or missing path.
            selected.add(relative)
        # Reviewed protobuf code embeds OUT_DIR bytes from tracked schemas.
        # Reviewed macro tests contain escaped fixture code. A source change or
        # any other include syntax selects the complete source tree instead.
        remaining = INCLUDE.sub("", source)
        if INCLUDE_NAME.search(remaining) and digest(
            path.read_bytes()
        ) != REVIEWED_DYNAMIC_INCLUDES.get(name):
            complete_tree = True
    if complete_tree:
        selected = set(names)
    selected.update(glossary_inputs)
    records = {name: file_record(root / name, root) for name in sorted(selected)}
    identity = {
        "sha256": digest(encoded(records)),
        "completeTree": complete_tree,
        "readerContract": readers,
        "cacheEligible": known_readers and not complete_tree and not glossary_inputs,
    }
    return {"identity": identity, "files": records}


def source_identity(root, kind="rust"):
    return source_snapshot(root, kind)["identity"]


def probe(command, root):
    result = subprocess.run(command, cwd=root, capture_output=True, check=True)
    executable = Path(shutil.which(command[0]) or command[0]).resolve()
    return {
        "command": command,
        "executable": str(executable),
        "binary": digest(executable.read_bytes()),
        "stdout": result.stdout.decode(errors="replace"),
        "stderr": result.stderr.decode(errors="replace"),
    }


def tool_identity(kind, root):
    compiler = os.environ.get("RUSTC") or os.environ.get("CARGO_BUILD_RUSTC") or "rustc"
    documenter = (
        os.environ.get("RUSTDOC") or os.environ.get("CARGO_BUILD_RUSTDOC") or "rustdoc"
    )
    protobuf = os.environ.get("PROTOC") or "protoc"
    commands = [
        [compiler, "-vV"],
        [documenter, "-vV"],
        ["cargo", "--version"],
        [protobuf, "--version"],
    ]
    if kind == "swift":
        commands += [["swift", "--version"], ["xcodebuild", "-version"]]
        commands += [
            ["xcrun", "--sdk", sdk, "--show-sdk-version"]
            for sdk in ("iphoneos", "iphonesimulator")
        ]
    if kind == "kotlin":
        commands += [["java", "-version"]]
    tools = [probe(command, root) for command in commands]
    flags = {
        name: value
        for name, value in os.environ.items()
        if name
        in (
            "RUSTFLAGS",
            "RUSTDOCFLAGS",
            "CARGO_ENCODED_RUSTFLAGS",
            "SDKROOT",
            "CARGO_ENCODED_RUSTDOCFLAGS",
            "CARGO_BUILD_TARGET",
            "CARGO_BUILD_RUSTFLAGS",
            "CARGO_BUILD_RUSTDOC",
            "DEVELOPER_DIR",
            "MACOSX_DEPLOYMENT_TARGET",
            "IPHONEOS_DEPLOYMENT_TARGET",
            "RUSTC",
            "RUSTC_WRAPPER",
            "RUSTC_WORKSPACE_WRAPPER",
            "CARGO_BUILD_RUSTC",
            "RUSTDOC",
            "PROTOC",
            "PROTOC_INCLUDE",
            "CC",
            "CXX",
            "AR",
            "RANLIB",
            "CFLAGS",
            "CXXFLAGS",
            "LDFLAGS",
            "JAVA_HOME",
            "JAVA_TOOL_OPTIONS",
            "GRADLE_OPTS",
            "XMTP_TEST_LOGGING",
            "SQLX_OFFLINE",
            "NIX_GIT_SHA",
            "CI",
            "NIX_CFLAGS_COMPILE",
            "NIX_CFLAGS_LINK",
            "NIX_LDFLAGS",
            "NIX_CC",
        )
        or name.startswith(
            (
                "CARGO_TARGET_",
                "CARGO_PROFILE_",
                "CC_",
                "CXX_",
                "AR_",
                "RANLIB_",
                "CFLAGS_",
                "CXXFLAGS_",
                "LDFLAGS_",
                "BINDGEN_EXTRA_CLANG_ARGS",
                "OPENSSL_",
            )
        )
    }
    return {
        "tools": tools,
        "flags": flags,
        "toolInputs": compiler_tool_inputs(flags),
        "runner": {
            "system": platform.system(),
            "arch": platform.machine(),
            "release": platform.release(),
            **{
                name: os.environ.get(name)
                for name in ("RUNNER_OS", "RUNNER_ARCH", "ImageOS", "ImageVersion")
            },
        },
    }


def is_nix_path(value):
    # Store paths name pinned build inputs. Mutable caller paths do not have
    # that contract, even when their current version string matches.
    try:
        return str(Path(value).resolve()).startswith("/nix/store/")
    except (OSError, RuntimeError):
        return False


def is_tool_flag(name):
    return (
        name in ("CC", "CXX", "AR", "RANLIB")
        or name.startswith(("CC_", "CXX_", "AR_", "RANLIB_"))
        or name.startswith("CARGO_TARGET_")
        and name.endswith("_LINKER")
    )


def compiler_tool_inputs(flags):
    records = {}
    for name, value in flags.items():
        if not is_tool_flag(name) or not value:
            continue
        words = shlex.split(value)
        if len(words) != 1:
            records[name] = {"unparsed": value}
            continue
        executable = Path(shutil.which(words[0]) or words[0]).resolve()
        records[name] = {"path": str(executable)}
        if executable.is_file():
            records[name]["sha256"] = digest(executable.read_bytes())
    return records


def flag_contract(kind, environment):
    unsafe = []
    flags = environment.get("flags", {})
    tools = environment.get("toolInputs", {})
    booleans = {
        "CI",
        "XMTP_TEST_LOGGING",
        "SQLX_OFFLINE",
        "OPENSSL_NO_VENDOR",
        "OPENSSL_STATIC",
    }
    for name, value in flags.items():
        if not value:
            continue
        if name in booleans and value in ("0", "1", "true", "false"):
            continue
        if name in (
            "MACOSX_DEPLOYMENT_TARGET",
            "IPHONEOS_DEPLOYMENT_TARGET",
        ) and re.fullmatch(r"\d+(?:\.\d+)*", value):
            continue
        if name == "NIX_GIT_SHA" and re.fullmatch(r"[0-9a-f]{7,40}", value):
            continue
        if name == "CARGO_BUILD_TARGET" and re.fullmatch(r"[A-Za-z0-9_.-]+", value):
            continue
        if kind != "kotlin" and name in (
            "JAVA_HOME",
            "JAVA_TOOL_OPTIONS",
            "GRADLE_OPTS",
        ):
            continue
        if (
            name in ("SDKROOT", "DEVELOPER_DIR", "PROTOC_INCLUDE", "JAVA_HOME")
            or name.endswith("_DIR")
            and name.startswith("OPENSSL_")
        ):
            if is_nix_path(value):
                continue
        if name == "NIX_CC" and is_nix_path(value):
            continue
        if name in (
            "NIX_CFLAGS_COMPILE",
            "NIX_CFLAGS_LINK",
            "NIX_LDFLAGS",
        ) and pinned_nix_flags(value):
            continue
        if is_tool_flag(name):
            record = tools.get(name, {})
            if record.get("sha256") and is_nix_path(record.get("path", "")):
                continue
        if name in (
            "RUSTC",
            "CARGO_BUILD_RUSTC",
            "RUSTDOC",
            "CARGO_BUILD_RUSTDOC",
            "PROTOC",
        ):
            executable = Path(shutil.which(value) or value).resolve()
            if executable.is_file() and is_nix_path(str(executable)):
                continue
        # This is the pinned wasm shell's built-in header path. No caller file,
        # response file, or custom sysroot option is allowed by this rule.
        if name == "CFLAGS_wasm32_unknown_unknown" and re.fullmatch(
            r"-I (/nix/store/[a-z0-9]{32}-[^ ]+/lib/clang/[0-9.]+/include)", value
        ):
            if is_nix_path(value[3:]):
                continue
        # Unknown flags can name a file without changing the flag string when
        # that file changes. Build current references; do not reuse or save them.
        unsafe.append(name)
    return {"cacheEligible": not unsafe, "unreviewed": sorted(unsafe)}


def pinned_nix_flags(value):
    words = shlex.split(value)
    index = 0
    while index < len(words):
        word = words[index]
        if word in (
            "-I",
            "-L",
            "-F",
            "-isystem",
            "-iframework",
            "-isysroot",
            "--sysroot",
        ):
            index += 1
            if index >= len(words) or not is_nix_path(words[index]):
                return False
        elif word.startswith(("-I/nix/store/", "-L/nix/store/", "-F/nix/store/")):
            if not is_nix_path(word[2:]):
                return False
        elif word.startswith("--sysroot=/nix/store/"):
            if not is_nix_path(word.split("=", 1)[1]):
                return False
        elif re.fullmatch(r"-frandom-seed=[A-Za-z0-9]+", word):
            pass
        elif word.startswith("-fmacro-prefix-map="):
            mapping = word.split("=", 1)[1].split("=", 1)
            if len(mapping) != 2 or not all(is_nix_path(path) for path in mapping):
                return False
        elif word not in (
            "-fno-strict-overflow",
            "-Wformat",
            "-Wformat-security",
            "-Werror=format-security",
            "-fstack-protector-strong",
            "-fno-plt",
            "-O2",
            "-O3",
            "-D_FORTIFY_SOURCE=2",
            "-D_FORTIFY_SOURCE=3",
        ):
            return False
        index += 1
    return True


def supported_cargo_flags(value):
    words = shlex.split(value) if isinstance(value, str) else value
    if not isinstance(words, list) or any(not isinstance(word, str) for word in words):
        return False
    index = 0
    while index < len(words):
        word = words[index]
        if (
            word == "--cfg"
            and index + 1 < len(words)
            and re.fullmatch(
                r'[A-Za-z_][A-Za-z_0-9]*(?:="[A-Za-z_0-9-]+")?', words[index + 1]
            )
        ):
            index += 2
        elif (
            word == "-C"
            and index + 1 < len(words)
            and re.fullmatch(
                r"target-feature=[+-][A-Za-z_0-9-]+(?:,[+-][A-Za-z_0-9-]+)*",
                words[index + 1],
            )
        ):
            index += 2
        elif re.fullmatch(r"-Clink-arg=-Wl,-z,max-page-size=[0-9]+", word):
            index += 1
        else:
            return False
    return True


def ambient_contract(kind, root):
    unsafe = []
    configs = {}
    try:
        configs, effective = cargo_inputs.cargo_config(root)
        if any(not name.startswith("workspace:") for name in configs):
            unsafe.append("externalCargoConfig")
        if effective.get("env"):
            unsafe.append("cargoEnvironment")
        build = effective.get("build", {})
        targets = list(effective.get("target", {}).values())
        for settings in (build, *targets):
            if not isinstance(settings, dict):
                unsafe.append("unknownCargoOptions")
                continue
            if settings.get("rustdocflags"):
                unsafe.append("cargoRustdocFlags")
            if settings.get("rustflags") and not supported_cargo_flags(
                settings["rustflags"]
            ):
                unsafe.append("cargoFileFlags")
        if any(
            build.get(name)
            for name in (
                "rustc",
                "rustdoc",
                "rustc-wrapper",
                "rustc-workspace-wrapper",
                "dep-info-basedir",
            )
        ):
            unsafe.append("cargoCompilerOverride")
    except (OSError, ValueError, TypeError, AttributeError):
        unsafe.append("unknownCargoConfig")
    gradle_inputs = {}
    if kind == "kotlin":
        home = Path(os.environ.get("GRADLE_USER_HOME") or Path.home() / ".gradle")
        candidates = [
            home / name
            for name in ("gradle.properties", "init.gradle", "init.gradle.kts")
        ]
        candidates.extend((home / "init.d").rglob("*"))
        for path in candidates:
            if path.is_file():
                gradle_inputs[path.relative_to(home).as_posix()] = digest(
                    path.read_bytes()
                )
        if gradle_inputs:
            unsafe.append("globalGradleConfig")
        # The current wrapper distribution and installation inputs are not qualified.
        unsafe.append("unqualifiedGradleTools")
    if kind == "swift":
        # Apple tools and global SwiftPM inputs do not have an immutable contract.
        unsafe.append("unqualifiedSwiftTools")
    return {
        "cacheEligible": not unsafe,
        "unreviewed": sorted(set(unsafe)),
        "cargoConfigs": configs,
        "gradleConfigs": gradle_inputs,
    }


def identity(kind, root):
    value = {
        "schema": SCHEMA,
        "kind": kind,
        "source": source_identity(root, kind),
        "environment": tool_identity(kind, root),
        "ambientContract": ambient_contract(kind, root),
    }
    value["flagContract"] = flag_contract(kind, value["environment"])
    value["cacheEligible"] = (
        value["source"]["cacheEligible"]
        and value["flagContract"]["cacheEligible"]
        and value["ambientContract"]["cacheEligible"]
    )
    value["key"] = f"docs-reference-v{SCHEMA}-{kind}-" + digest(encoded(value))
    return value


def inventory(folder, skip_receipt=True):
    if not folder.is_dir():
        raise ValueError(f"Reference output is missing: {folder}")
    files = {}
    for path in sorted(folder.rglob("*")):
        if path.is_symlink():
            raise ValueError(f"Reference output has a symbolic link: {path}")
        if path.is_file() and not (skip_receipt and path == folder / RECEIPT):
            files[path.relative_to(folder).as_posix()] = digest(path.read_bytes())
    if not files:
        raise ValueError("Reference output is empty")
    return files


def check_entrypoints(kind, folder):
    entries = {
        "rust": ("xmtp_mls/index.html",),
        "kotlin": ("index.html",),
        "swift": ("index.html", "documentation/xmtpsdk/index.html"),
    }[kind]
    for name in entries:
        if not (folder / name).is_file() or not (folder / name).stat().st_size:
            raise ValueError(f"Reference entrypoint is missing or empty: {name}")


def stamp(folder, expected, current):
    if expected != current:
        raise ValueError("Reference inputs changed during the build")
    check_entrypoints(current["kind"], folder)
    record = {"identity": current, "files": inventory(folder)}
    (folder / RECEIPT).write_bytes(encoded(record) + b"\n")


def verify(folder, current):
    try:
        record = json.loads((folder / RECEIPT).read_bytes())
    except (OSError, ValueError):
        raise ValueError("Reference cache receipt is missing or invalid") from None
    if record.get("identity") != current:
        raise ValueError("Reference cache does not match current inputs")
    if not current.get("cacheEligible", False):
        raise ValueError("Reference inputs are not eligible for cache reuse")
    check_entrypoints(current["kind"], folder)
    if record.get("files") != inventory(folder):
        raise ValueError("Reference cache output bytes changed")


def safe_identity(value):
    result = dict(value)
    environment = value["environment"]
    result["environment"] = {
        "flags": {
            name: digest(encoded(flag))
            for name, flag in environment.get("flags", {}).items()
        },
        "tools": [
            {name: digest(encoded(field)) for name, field in tool.items()}
            for tool in environment.get("tools", [])
        ],
        "toolInputs": {
            name: digest(encoded(field))
            for name, field in environment.get("toolInputs", {}).items()
        },
        "runner": environment["runner"],
    }
    return result


def write_baseline(state, current):
    Path(str(state) + ".expected-safe.json").write_bytes(
        encoded(safe_identity(current)) + b"\n"
    )
    files = {
        name: digest(encoded(value))
        for name, value in source_snapshot(ROOT, current["kind"])["files"].items()
    }
    Path(str(state) + ".inputs.json").write_bytes(encoded(files) + b"\n")


def write_mismatch(state, expected, current):
    changes = [
        name
        for name in expected
        if name != "key" and expected.get(name) != current.get(name)
    ]
    files = {
        name: digest(encoded(value))
        for name, value in source_snapshot(ROOT, current["kind"])["files"].items()
    }
    try:
        before = json.loads(Path(str(state) + ".inputs.json").read_bytes())
    except (OSError, ValueError):
        before = {}
    changed_files = [
        name
        for name in sorted(set(before) | set(files))
        if before.get(name) != files.get(name)
    ]
    report = {
        "components": changes,
        "changedSourcePaths": changed_files,
        "expected": safe_identity(expected),
        "current": safe_identity(current),
    }
    Path(str(state) + ".failure-safe.json").write_bytes(encoded(report) + b"\n")
    print(
        "Reference cache failed: changed components: " + ", ".join(changes),
        file=sys.stderr,
    )
    if changed_files:
        print(
            "Reference cache failed: changed source paths: "
            + ", ".join(changed_files[:20]),
            file=sys.stderr,
        )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("key", "stamp", "verify"))
    parser.add_argument("--kind", choices=("rust", "swift", "kotlin"), required=True)
    parser.add_argument("--folder", type=Path)
    parser.add_argument("--state", type=Path, required=True)
    parser.add_argument("--github-output", type=Path)
    args = parser.parse_args()
    current = identity(args.kind, ROOT)
    if args.action == "key":
        args.state.write_bytes(encoded(current) + b"\n")
        write_baseline(args.state, current)
        if args.github_output:
            with args.github_output.open("a") as output:
                output.write(f"key={current['key']}\n")
                output.write(
                    f"cache-eligible={str(current['cacheEligible']).lower()}\n"
                )
        print(current["key"])
        return
    if not args.folder:
        raise ValueError("Reference output folder is required")
    expected = json.loads(args.state.read_bytes())
    if current != expected:
        write_mismatch(args.state, expected, current)
        raise ValueError("Reference inputs changed after cache selection")
    if args.action == "stamp":
        stamp(args.folder, expected, current)
    else:
        verify(args.folder, current)
    print(f"Reference {args.action} passed: {args.kind}")


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        print(f"Reference cache failed: {error}", file=sys.stderr)
        sys.exit(1)
