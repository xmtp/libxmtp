#!/usr/bin/env python3
"""Check config provenance, mobile features, and Android target tools."""

import argparse
import importlib.util
import json
import os
import signal
from pathlib import Path
import tempfile
import subprocess
import shutil
import zipfile
from unittest.mock import Mock, patch
import unittest


def load(name, file):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(file))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


artifacts = load("artifacts", "sdk-artifacts.py")
receipt = load("receipt", "record-generated.py")
mobile = load("mobile", "mobile-package.py")


class PackagingTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        (self.root / "crates/xmtp_sdk").mkdir(parents=True)
        (self.root / "apps/xmtp_sdk_bindgen").mkdir(parents=True)
        (self.root / "Cargo.toml").write_text("fixture manifest")
        (self.root / "apps/xmtp_sdk_bindgen/template.txt").write_text(
            "fixture template"
        )
        self.config = self.root / "crates/xmtp_sdk/uniffi.toml"
        self.config.write_text("fixture configuration")
        self.calls = []
        self.patches = [
            patch.object(artifacts, "ROOT", self.root),
            patch.object(receipt.artifacts, "ROOT", self.root),
            patch.object(artifacts, "build_context", return_value="fixture compiler"),
            patch.object(artifacts, "run", side_effect=self.command),
            patch.object(mobile.artifacts, "ROOT", self.root),
        ]
        for item in self.patches:
            item.start()
        self.args = argparse.Namespace(
            artifacts=self.root / "compiled",
            targets=("swift",),
            out=self.root / "generated",
            features="",
            rust_target="",
            skip_bindgen=False,
            reuse_bindgen=None,
            profile="release",
            no_format=True,
        )

    def tearDown(self):
        for item in reversed(self.patches):
            item.stop()
        self.temporary.cleanup()

    def seed_android_dependency_inputs(self):
        project = self.root / "crates/xmtp_sdk/packaging/android"
        for name in (
            "gradle.lockfile",
            "buildscript-gradle.lockfile",
            "gradle/verification-metadata.xml",
        ):
            file = project / name
            file.parent.mkdir(parents=True, exist_ok=True)
            file.write_text("fixture dependency input")

    def prepare_mobile_stage(self, target):
        self.args.targets = ("swift",) if target == "ios" else ("kotlin",)
        artifacts.build(self.args)
        artifacts.render(self.args)
        triples = mobile.IOS if target == "ios" else tuple(mobile.ANDROID.values())
        for triple in triples:
            args = argparse.Namespace(**vars(self.args))
            args.artifacts = self.root / "mobile" / triple
            args.rust_target = triple
            args.skip_bindgen = True
            artifacts.build(args)
        if target == "ios":
            generated = self.args.out / "swift"
            (generated / "xmtp_sdkFFI.h").write_text("fixture header")
            (generated / "xmtp_sdkFFI.modulemap").write_text("fixture module")
            (generated / "runtime").mkdir()
            (generated / "runtime/Client.swift").write_text("fixture runtime")
        if target == "android":
            self.seed_android_dependency_inputs()
        output = self.root / "products" / target
        if output.exists():
            shutil.rmtree(output)
        output.mkdir(parents=True)
        (output / "previous.txt").write_bytes(b"prior valid product")
        return output

    def mobile_tool(self, command, **kwargs):
        if command[0] == "xcodebuild":
            output = Path(command[command.index("-output") + 1])
            output.mkdir(parents=True)
            (output / "library").write_text("fixture xcframework")
        else:
            output = self.root / "crates/xmtp_sdk/packaging/android/build/outputs/aar"
            output.mkdir(parents=True, exist_ok=True)
            with zipfile.ZipFile(output / "xmtp-sdk-stage-release.aar", "w") as archive:
                archive.writestr("classes.jar", b"fixture classes")
                for abi in mobile.ANDROID:
                    archive.writestr(f"jni/{abi}/libxmtp_sdk.so", b"fixture native")

    def assemble_mobile(self, target, tool=None):
        if not isinstance(tool, Mock):
            tool = Mock(side_effect=tool or self.mobile_tool)
        with (
            patch.object(mobile, "ROOT", self.root),
            patch.object(mobile, "run", tool),
            patch.object(
                mobile.sys,
                "argv",
                [
                    "mobile-package.py",
                    "stage",
                    target,
                    "--generated",
                    str(self.args.out),
                    "--artifacts",
                    str(self.root / "mobile"),
                    "--out",
                    str(self.root / "products"),
                ],
            ),
        ):
            mobile.main()

    def product_files(self, output):
        return {
            str(path.relative_to(output)): path.read_bytes()
            for path in output.rglob("*")
            if path.is_file()
        }

    def test_android_dependency_inputs_are_required_before_tool_use(self):
        for name in (
            "gradle.lockfile",
            "buildscript-gradle.lockfile",
            "gradle/verification-metadata.xml",
        ):
            with self.subTest(name=name):
                output = self.prepare_mobile_stage("android")
                previous = self.product_files(output)
                project = self.root / "crates/xmtp_sdk/packaging/android"
                (project / name).unlink()
                tool = Mock(side_effect=self.mobile_tool)
                with self.assertRaisesRegex(
                    ValueError, "Android dependency input missing"
                ):
                    self.assemble_mobile("android", tool)
                tool.assert_not_called()
                self.assertEqual(self.product_files(output), previous)
                self.assertEqual(list(output.parent.glob(".sdk-mobile-stage-*")), [])

    def test_android_stage_uses_strict_read_only_dependency_inputs(self):
        self.prepare_mobile_stage("android")

        def tool(command, **kwargs):
            self.assertEqual(command[0], "sdks/android/gradlew")
            self.assertIn("--dependency-verification=strict", command)
            self.assertIn("--max-workers=2", command)
            self.assertFalse(
                any(
                    arg.startswith(
                        (
                            "--write-locks",
                            "--update-locks",
                            "--write-verification-metadata",
                        )
                    )
                    for arg in command
                )
            )
            self.mobile_tool(command, **kwargs)

        self.assemble_mobile("android", tool)

    def test_mobile_late_tool_failure_preserves_prior_and_cleans_fresh_stage(self):
        for target in ("ios", "android"):
            with self.subTest(target=target):
                output = self.prepare_mobile_stage(target)
                before = self.product_files(output)

                def fail(command, **kwargs):
                    self.mobile_tool(command, **kwargs)
                    raise subprocess.CalledProcessError(1, command)

                with self.assertRaises(subprocess.CalledProcessError):
                    self.assemble_mobile(target, fail)
                self.assertEqual(self.product_files(output), before)
                self.assertEqual(list(output.parent.glob(".sdk-mobile-stage-*")), [])
                shutil.rmtree(output)
                with self.assertRaises(subprocess.CalledProcessError):
                    self.assemble_mobile(target, fail)
                self.assertFalse(output.exists())
                self.assertEqual(list(output.parent.glob(".sdk-mobile-stage-*")), [])

    def test_mobile_archive_failure_preserves_prior_stage(self):
        output = self.prepare_mobile_stage("android")
        before = self.product_files(output)

        def missing_abi(command, **kwargs):
            self.mobile_tool(command, **kwargs)
            archive = (
                self.root
                / "crates/xmtp_sdk/packaging/android/build/outputs/aar/xmtp-sdk-stage-release.aar"
            )
            with zipfile.ZipFile(archive, "w") as broken:
                broken.writestr("classes.jar", b"fixture classes")

        with self.assertRaisesRegex(ValueError, "AAR missing ABI"):
            self.assemble_mobile("android", missing_abi)
        self.assertEqual(self.product_files(output), before)
        self.assertEqual(list(output.parent.glob(".sdk-mobile-stage-*")), [])

    def test_mobile_success_replaces_prior_with_checked_product(self):
        for target in ("ios", "android"):
            with self.subTest(target=target):
                output = self.prepare_mobile_stage(target)
                self.assemble_mobile(target)
                self.assertFalse((output / "previous.txt").exists())
                contract = json.loads((output / "sdk-contract.json").read_text())
                self.assertTrue(contract["assets"])
                for name, expected in contract["assets"].items():
                    self.assertEqual(artifacts.digest(output / name), expected)
                self.assertEqual(list(output.parent.glob(".sdk-mobile-stage-*")), [])

    def test_mobile_interruptions_preserve_prior_bytes(self):
        replace = os.replace
        for target in ("ios", "android"):
            for kind in ("sigint", "exit"):
                for phase in ("before-backup", "after-backup", "after-promotion"):
                    with self.subTest(target=target, kind=kind, phase=phase):
                        output = self.prepare_mobile_stage(target).resolve()
                        before = self.product_files(output)
                        fired = False

                        def interrupt(source, destination):
                            nonlocal fired
                            backup = Path(source) == output
                            promotion = Path(source).name == "product"
                            selected = (backup and phase != "after-promotion") or (
                                promotion and phase == "after-promotion"
                            )
                            if not fired and selected and phase == "before-backup":
                                fired = True
                                if kind == "sigint":
                                    os.kill(os.getpid(), signal.SIGINT)
                                raise SystemExit(71)
                            replace(source, destination)
                            if not fired and selected:
                                fired = True
                                if kind == "sigint":
                                    os.kill(os.getpid(), signal.SIGINT)
                                raise SystemExit(71)

                        expected = KeyboardInterrupt if kind == "sigint" else SystemExit
                        with patch.object(mobile.os, "replace", side_effect=interrupt):
                            with self.assertRaises(expected) as error:
                                self.assemble_mobile(target)
                        self.assertTrue(fired)
                        if kind == "exit":
                            self.assertEqual(error.exception.code, 71)
                        self.assertEqual(self.product_files(output), before)
                        self.assertEqual(
                            list(output.parent.glob(".sdk-mobile-stage-*")), []
                        )

    def test_mobile_interrupted_rollback_preserves_original_failure(self):
        replace = os.replace
        for target in ("ios", "android"):
            for kind in ("sigint", "exit"):
                for phase in ("before-rollback", "after-rollback"):
                    with self.subTest(target=target, kind=kind, phase=phase):
                        output = self.prepare_mobile_stage(target).resolve()
                        before = self.product_files(output)

                        def invoke():
                            self.assemble_mobile(target)

                        files = self.product_files
                        prefix = ".sdk-mobile-stage-*"
                        failure = OSError("promotion failed")
                        fired = False

                        def interrupt(source, destination):
                            nonlocal fired
                            if Path(source).name == "product":
                                raise failure
                            rollback = Path(source).name == "previous"
                            if rollback and phase == "before-rollback":
                                fired = True
                                if kind == "sigint":
                                    os.kill(os.getpid(), signal.SIGINT)
                                raise SystemExit(71)
                            replace(source, destination)
                            if rollback and phase == "after-rollback":
                                fired = True
                                if kind == "sigint":
                                    os.kill(os.getpid(), signal.SIGINT)
                                raise SystemExit(71)

                        with patch.object(os, "replace", side_effect=interrupt):
                            with self.assertRaises(BaseException) as error:
                                invoke()
                        self.assertTrue(fired)
                        self.assertIs(error.exception, failure)
                        stages = list(output.parent.glob(prefix))
                        if phase == "before-rollback":
                            self.assertFalse(output.exists())
                            backup = Path(
                                error.exception.__notes__[0].split("preserved at ", 1)[
                                    1
                                ]
                            )
                            self.assertTrue(backup.is_absolute())
                            self.assertEqual(files(backup), before)
                            invoke()
                            self.assertEqual(files(backup), before)
                            recovery = self.root / "manual-recovery"
                            if recovery.exists():
                                shutil.rmtree(recovery)
                            os.replace(backup, recovery)
                            self.assertEqual(files(recovery), before)
                            shutil.rmtree(stages[0])
                        else:
                            self.assertEqual(files(output), before)
                            self.assertEqual(stages, [])

    def test_mobile_promotion_failures_restore_or_preserve_prior_bytes(self):
        replace = os.replace
        for target in ("ios", "android"):
            for failures in ((1,), (2,), (2, 3)):
                with self.subTest(target=target, failures=failures):
                    output = self.prepare_mobile_stage(target)
                    before = self.product_files(output)
                    calls = []

                    def fail(source, destination):
                        calls.append((Path(source), Path(destination)))
                        if len(calls) in failures:
                            raise OSError("fixture rename failure")
                        replace(source, destination)

                    with patch.object(mobile.os, "replace", side_effect=fail):
                        with self.assertRaisesRegex(
                            OSError, "fixture rename failure|previous product preserved"
                        ) as error:
                            self.assemble_mobile(target)
                    if failures == (2, 3):
                        self.assertFalse(output.exists())
                        stages = list(output.parent.glob(".sdk-mobile-stage-*"))
                        self.assertEqual(len(stages), 1)
                        self.assertEqual(
                            error.exception.__notes__[0],
                            f"previous product preserved at {(stages[0] / 'previous').resolve()}",
                        )
                        self.assertEqual(
                            self.product_files(stages[0] / "previous"), before
                        )
                        shutil.rmtree(stages[0])
                    else:
                        self.assertEqual(self.product_files(output), before)
                        self.assertEqual(
                            list(output.parent.glob(".sdk-mobile-stage-*")), []
                        )
                        shutil.rmtree(output)

    def test_bridge_fixture_builds_use_rust_shell(self):
        source = Path(__file__).with_name("run-bridge-conformance").read_text()
        names = ("prepare-bridge-panic-fixture", "prepare-pure-codec-fixture")
        commands = [
            line
            for line in source.splitlines()
            if any("bash crates/xmtp_sdk/dev/" + name in line for name in names)
        ]
        self.assertEqual(len(commands), 2)
        wrapper = self.root / "dev/nix-shell"
        wrapper.parent.mkdir(parents=True)
        wrapper.write_text(
            '#!/usr/bin/env bash\nset -euo pipefail\nexport XMTP_DEV_SHELL="$NIX_DEVSHELL"\nexec bash -euc "$1"\n'
        )
        wrapper.chmod(0o755)
        trace = self.root / "fixture-shells.txt"
        for name in names:
            fixture = self.root / "crates/xmtp_sdk/dev" / name
            fixture.parent.mkdir(parents=True, exist_ok=True)
            fixture.write_text(
                'printf "%s:%s\\n" "'
                + name
                + '" "$XMTP_DEV_SHELL" >> "$SDK_SHELL_TRACE"\n'
            )
        for command in commands:
            subprocess.run(
                ["bash", "-euc", command],
                cwd=self.root,
                check=True,
                env=dict(
                    os.environ,
                    XMTP_DEV_SHELL="js",
                    NIX_DEVSHELL="js",
                    SDK_SHELL_TRACE=str(trace),
                ),
            )
        self.assertEqual(
            trace.read_text().splitlines(), [name + ":rust" for name in names]
        )

    def command(self, command, **kwargs):
        self.calls.append(command)
        if command[0] == "dev/agent-run":
            folder = Path(kwargs["env"]["CARGO_TARGET_DIR"])
            if "--target" in command:
                folder /= command[command.index("--target") + 1]
            folder /= "debug" if "xmtp-sdk-bindgen" in command else "release"
            folder.mkdir(parents=True, exist_ok=True)
            for name in (
                "libxmtp_sdk.a",
                "libxmtp_sdk.dylib",
                "libxmtp_sdk.so",
                "xmtp-sdk-bindgen",
            ):
                (folder / name).write_text("fixture binary")
        else:
            folder = Path(command[command.index("--out") + 1])
            folder.mkdir(parents=True, exist_ok=True)
            language = command[command.index("--language") + 1]
            name = "xmtp_sdk.kt" if language == "kotlin" else "xmtp_sdk.swift"
            (folder / name).write_text("fixture binding")

    def native_receipts(self, target="ios"):
        native = json.loads((self.args.artifacts / "artifacts.json").read_text())[
            "artifacts"
        ]["native"]
        native["features"] = ""
        for triple in mobile.IOS if target == "ios" else tuple(mobile.ANDROID.values()):
            folder = self.root / "mobile" / triple
            folder.mkdir(parents=True, exist_ok=True)
            (folder / "artifacts.json").write_text(
                json.dumps({"artifacts": {"native": dict(native, target=triple)}})
            )

    def test_live_compile_inputs_invalidate_build_and_recorded_source(self):
        for name in artifacts.COMPILE_INPUTS:
            with self.subTest(input=name):
                path = self.root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("original embedded data")
                artifacts.build(self.args)
                artifacts.render(self.args)
                before = artifacts.source_hash()
                binaries = {
                    role: Path(next(iter(item["files"])))
                    for role, item in json.loads(
                        (self.args.artifacts / "artifacts.json").read_text()
                    )["artifacts"].items()
                }
                receipt.record(self.args.out, binaries)
                old_record = json.loads(
                    (self.args.out / "swift/sdk-contract.json").read_text()
                )
                before_calls = len(self.calls)
                path.write_text("changed embedded data only")
                self.assertNotEqual(artifacts.source_hash(), before)
                with self.assertRaisesRegex(ValueError, "source contract mismatch"):
                    artifacts.render(self.args)
                self.assertEqual(len(self.calls), before_calls)
                receipt.record(self.args.out, binaries)
                new_record = json.loads(
                    (self.args.out / "swift/sdk-contract.json").read_text()
                )
                self.assertNotEqual(
                    new_record["artifact"]["source"], old_record["artifact"]["source"]
                )
                artifacts.build(self.args)
                self.assertEqual(len(self.calls), before_calls + 2)
                path.write_text("original embedded data")
                artifacts.build(self.args)
                artifacts.render(self.args)
                self.assertEqual(artifacts.source_hash(), before)
                print(
                    "Embedded input changed source/build admission and restored:", name
                )

    def test_swapped_mobile_targets_rejected_before_assembly(self):
        for target in ("android", "ios"):
            self.args.targets = ("kotlin",) if target == "android" else ("swift",)
            artifacts.build(self.args)
            artifacts.render(self.args)
            self.native_receipts(target)
            triples = (
                tuple(mobile.ANDROID.values()) if target == "android" else mobile.IOS
            )
            for triple in triples:
                wrong = triples[0] if triple != triples[0] else triples[-1]
                folder = self.root / "mobile" / triple / "artifacts.json"
                original = folder.read_text()
                folder.write_text(
                    (self.root / "mobile" / wrong / "artifacts.json").read_text()
                )
                with self.subTest(platform=target, expected=triple, wrong=wrong):
                    with (
                        patch.object(
                            mobile.sys,
                            "argv",
                            [
                                "mobile-package.py",
                                "stage",
                                target,
                                "--generated",
                                str(self.args.out),
                                "--artifacts",
                                str(self.root / "mobile"),
                                "--out",
                                str(self.root / "products"),
                            ],
                        ),
                        patch.object(mobile, "run") as assembly,
                    ):
                        with self.assertRaisesRegex(
                            ValueError, "mobile binding contract mismatch"
                        ):
                            mobile.main()
                        assembly.assert_not_called()
                    self.assertFalse((self.root / "products").exists())
                folder.write_text(original)
                mobile.preflight(self.args.out, self.root / "mobile", target)
                print(
                    "Swapped target rejected before assembly; restored:", target, triple
                )

    def test_git_and_filtered_source_have_same_fingerprint(self):
        subprocess.run(["git", "init", "--quiet", str(self.root)], check=True)
        subprocess.run(["git", "-C", str(self.root), "add", "."], check=True)
        expected = (artifacts.source_hash(), artifacts.source_hash(True))
        (self.root / ".git").rename(self.root / ".git-hidden")
        self.assertEqual(
            (artifacts.source_hash(), artifacts.source_hash(True)), expected
        )

    def test_unqualified_compiler_inputs_change_native_cache_admission(self):
        names = ("CC", "CXX", "AR", "CFLAGS", "CXXFLAGS", "LDFLAGS")
        environment = {
            key: value for key, value in os.environ.items() if key not in names
        }
        self.patches[2].stop()
        try:
            with patch.object(
                artifacts.subprocess, "check_output", return_value=b"fixture rustc"
            ):
                for name in names:
                    with (
                        self.subTest(input=name),
                        patch.dict(os.environ, environment, clear=True),
                    ):
                        self.args.artifacts = self.root / name
                        baseline = artifacts.build_context()
                        artifacts.build(self.args)
                        before = json.loads(
                            (self.args.artifacts / "artifacts.json").read_text()
                        )
                        calls = len(self.calls)
                        os.environ[name] = "changed compiler input"
                        self.assertNotEqual(artifacts.build_context(), baseline)
                        artifacts.build(self.args)
                        after = json.loads(
                            (self.args.artifacts / "artifacts.json").read_text()
                        )
                        self.assertNotEqual(
                            before["artifacts"]["native"]["key"],
                            after["artifacts"]["native"]["key"],
                        )
                        self.assertEqual(len(self.calls), calls + 2)
        finally:
            self.patches[2].start()

    def test_cargo_compiler_inputs_change_native_cache_admission(self):
        names = ("MACOSX_DEPLOYMENT_TARGET", "RUSTC", "CARGO_BUILD_RUSTC")
        environment = {
            key: value for key, value in os.environ.items() if key not in names
        }
        compiler = self.root / "fixture-rustc"
        compiler.write_text("#!/bin/sh\nprintf 'fixture rustc version one\\n'\n")
        compiler.chmod(0o755)
        self.patches[2].stop()
        try:
            for name in names:
                with (
                    self.subTest(input=name),
                    patch.dict(os.environ, environment, clear=True),
                ):
                    self.args.artifacts = self.root / name
                    artifacts.build(self.args)
                    before = json.loads(
                        (self.args.artifacts / "artifacts.json").read_text()
                    )
                    calls = len(self.calls)
                    os.environ[name] = (
                        "11.0" if name == "MACOSX_DEPLOYMENT_TARGET" else str(compiler)
                    )
                    artifacts.build(self.args)
                    after = json.loads(
                        (self.args.artifacts / "artifacts.json").read_text()
                    )
                    self.assertNotEqual(
                        before["artifacts"]["native"]["key"],
                        after["artifacts"]["native"]["key"],
                    )
                    self.assertEqual(len(self.calls), calls + 2)
                    calls = len(self.calls)
                    artifacts.build(self.args)
                    self.assertEqual(len(self.calls), calls)
        finally:
            self.patches[2].start()

    def test_selected_compiler_version_changes_rebuild_without_path_change(self):
        environment = {
            key: value
            for key, value in os.environ.items()
            if key not in ("RUSTC", "CARGO_BUILD_RUSTC")
        }
        compiler = self.root / "fixture-rustc"
        compiler.write_text("#!/bin/sh\nprintf 'fixture rustc version one\\n'\n")
        compiler.chmod(0o755)
        self.patches[2].stop()
        try:
            for variable in ("RUSTC", "CARGO_BUILD_RUSTC"):
                with (
                    self.subTest(input=variable),
                    patch.dict(os.environ, environment, clear=True),
                ):
                    os.environ[variable] = str(compiler)
                    self.args.artifacts = self.root / variable
                    artifacts.build(self.args)
                    calls = len(self.calls)
                    compiler.write_text(
                        "#!/bin/sh\nprintf 'fixture rustc version two\\n'\n"
                    )
                    artifacts.build(self.args)
                    self.assertEqual(len(self.calls), calls + 2)
                    compiler.write_text(
                        "#!/bin/sh\nprintf 'fixture rustc version one\\n'\n"
                    )
        finally:
            self.patches[2].start()

    def test_selected_compiler_bytes_change_rebuild_with_same_version(self):
        compiler = self.root / "fixture-rustc"
        compiler.write_text(
            "#!/bin/sh\nprintf 'same version\\n'\n# original compiler\n"
        )
        compiler.chmod(0o755)
        self.patches[2].stop()
        try:
            with patch.dict(os.environ, {"RUSTC": str(compiler)}):
                artifacts.build(self.args)
                calls = len(self.calls)
                compiler.write_text(
                    "#!/bin/sh\nprintf 'same version\\n'\n# changed compiler\n"
                )
                artifacts.build(self.args)
                self.assertEqual(len(self.calls), calls + 2)
                calls = len(self.calls)
                artifacts.build(self.args)
                self.assertEqual(len(self.calls), calls)
        finally:
            self.patches[2].start()

    def test_rustc_override_has_precedence_over_cargo_build_rustc(self):
        self.patches[2].stop()
        try:
            with (
                patch.dict(
                    os.environ,
                    {"RUSTC": "selected-rustc", "CARGO_BUILD_RUSTC": "other-rustc"},
                ),
                patch.object(
                    artifacts.subprocess,
                    "check_output",
                    return_value=b"selected compiler",
                ) as probe,
            ):
                artifacts.build_context()
                probe.assert_called_once_with(["selected-rustc", "-vV"], cwd=self.root)
        finally:
            self.patches[2].start()

    def test_actual_mobile_stage_rejects_stale_binding_generator(self):
        def host_inputs():
            swift = self.args.out / "swift"
            (swift / "xmtp_sdkFFI.h").write_text("fixture header")
            (swift / "xmtp_sdkFFI.modulemap").write_text("fixture module")
            (swift / "runtime").mkdir(exist_ok=True)
            (swift / "runtime/Client.swift").write_text("fixture runtime")
            self.seed_android_dependency_inputs()

        self.args.targets = ("swift", "kotlin")
        artifacts.build(self.args)
        artifacts.render(self.args)
        host_inputs()
        for platform in ("ios", "android"):
            for triple in (
                mobile.IOS if platform == "ios" else tuple(mobile.ANDROID.values())
            ):
                args = argparse.Namespace(**vars(self.args))
                args.artifacts = self.root / "mobile" / triple
                args.rust_target = triple
                args.skip_bindgen = True
                artifacts.build(args)
            output = self.root / "products" / platform
            output.mkdir(parents=True)
            (output / "previous.bin").write_bytes(b"previous product")
        original_receipts = {
            path: path.read_bytes()
            for path in (self.root / "mobile").rglob("artifacts.json")
        }
        self.assertEqual(len(original_receipts), 6)
        (self.root / "apps/xmtp_sdk_bindgen/template.txt").write_text("new template")
        for platform in ("ios", "android"):
            output = self.root / "products" / platform
            before = self.product_files(output)
            with self.subTest(platform=platform):
                with self.assertRaisesRegex(ValueError, "binding generator mismatch"):
                    self.assemble_mobile(platform)
                self.assertEqual(self.product_files(output), before)
        before_calls = len(self.calls)
        artifacts.build(self.args)
        self.assertEqual(len(self.calls), before_calls + 1)
        self.assertIn("xmtp-sdk-bindgen", self.calls[-1])
        artifacts.render(self.args)
        host_inputs()
        for platform in ("ios", "android"):
            self.assemble_mobile(platform)
        for path, original in original_receipts.items():
            self.assertEqual(path.read_bytes(), original)

    def test_generator_only_change_reuses_verified_native_provenance(self):
        self.args.targets = ("swift", "kotlin")
        artifacts.build(self.args)
        artifacts.render(self.args)
        for platform in ("ios", "android"):
            self.native_receipts(platform)
            mobile.preflight(self.args.out, self.root / "mobile", platform)
        native_receipts = {
            path: path.read_bytes()
            for path in (self.root / "mobile").rglob("artifacts.json")
        }
        self.assertEqual(len(native_receipts), 6)
        manifest = self.args.artifacts / "artifacts.json"
        native = json.loads(manifest.read_text())["artifacts"]["native"]
        source = artifacts.source_hash()
        (self.root / "apps/xmtp_sdk_bindgen/template.txt").write_text(
            "changed generator only"
        )
        self.assertEqual(artifacts.source_hash(), source)
        calls = len(self.calls)
        with self.assertRaisesRegex(ValueError, "generator contract mismatch"):
            artifacts.render(self.args)
        self.assertEqual(len(self.calls), calls)
        artifacts.build(self.args)
        self.assertEqual(len(self.calls), calls + 1)
        self.assertIn("xmtp-sdk-bindgen", self.calls[-1])
        self.assertEqual(
            json.loads(manifest.read_text())["artifacts"]["native"], native
        )
        artifacts.render(self.args)
        binding = json.loads((self.args.out / "swift/sdk-contract.json").read_text())
        self.assertEqual(binding["artifact"], native)
        self.assertEqual(binding["generator"], artifacts.source_hash(True))
        for path, original_receipt in native_receipts.items():
            self.assertEqual(path.read_bytes(), original_receipt)
        for platform in ("ios", "android"):
            with self.subTest(platform=platform):
                _, admitted = mobile.preflight(
                    self.args.out, self.root / "mobile", platform
                )
                for triple, item in admitted.items():
                    self.assertEqual(
                        json.loads(
                            native_receipts[
                                self.root / "mobile" / triple / "artifacts.json"
                            ]
                        )["artifacts"]["native"],
                        item,
                    )
        for platform, triples in (
            ("ios", mobile.IOS),
            ("android", tuple(mobile.ANDROID.values())),
        ):
            path = self.root / "mobile" / triples[0] / "artifacts.json"
            original_receipt = path.read_bytes()
            for field, wrong in (
                ("source", "wrong source"),
                ("features", "conformance"),
                ("profile", "debug"),
                ("target", "wrong target"),
            ):
                changed = json.loads(original_receipt)
                changed["artifacts"]["native"][field] = wrong
                path.write_text(json.dumps(changed))
                with (
                    self.subTest(platform=platform, rejected_field=field),
                    self.assertRaisesRegex(
                        ValueError, "mobile binding contract mismatch"
                    ),
                ):
                    mobile.preflight(self.args.out, self.root / "mobile", platform)
                path.write_bytes(original_receipt)
        library = Path(next(iter(native["files"])))
        original = library.read_bytes()
        library.write_bytes(original + b"tampered native")
        calls = len(self.calls)
        with self.assertRaisesRegex(ValueError, "artifact mismatch"):
            artifacts.render(self.args)
        self.assertEqual(len(self.calls), calls)
        for platform in ("ios", "android"):
            with (
                self.subTest(platform=platform, rejected_field="native bytes"),
                self.assertRaisesRegex(ValueError, "artifact mismatch"),
            ):
                mobile.preflight(self.args.out, self.root / "mobile", platform)
        library.write_bytes(original)
        artifacts.render(self.args)
        for platform in ("ios", "android"):
            mobile.preflight(self.args.out, self.root / "mobile", platform)

    def test_configuration_source_invalidates_cached_generator(self):
        path = self.root / "crates/xmtp_configuration/src/lib.rs"
        path.parent.mkdir(parents=True)
        path.write_text('pub const WASM_VFS_DIRECTORY: &str = "original";')
        artifacts.build(self.args)
        artifacts.render(self.args)
        before = artifacts.source_hash(True)
        path.write_text('pub const WASM_VFS_DIRECTORY: &str = "changed";')
        self.assertNotEqual(artifacts.source_hash(True), before)
        with self.assertRaisesRegex(ValueError, "generator contract mismatch"):
            artifacts.render(self.args)
        artifacts.build(self.args)
        artifacts.render(self.args)

    def test_config_only_change_invalidates_cli_and_recorded_contract(self):
        artifacts.build(self.args)
        artifacts.render(self.args)
        self.native_receipts()
        original = self.config.read_text()
        before = json.loads((self.args.out / "swift/sdk-contract.json").read_text())
        binaries = {
            role: Path(next(iter(item["files"])))
            for role, item in json.loads(
                (self.args.artifacts / "artifacts.json").read_text()
            )["artifacts"].items()
        }
        receipt.record(self.args.out, binaries)
        recorded_before = json.loads(
            (self.args.out / "swift/sdk-contract.json").read_text()
        )
        self.config.write_text("changed configuration only")
        with self.assertRaisesRegex(ValueError, "generator contract mismatch"):
            artifacts.render(self.args)
        receipt.record(self.args.out, binaries)
        recorded_after = json.loads(
            (self.args.out / "swift/sdk-contract.json").read_text()
        )
        print(
            "Config-only contract before:",
            recorded_before["contract"],
            "after:",
            recorded_after["contract"],
        )
        self.assertNotEqual(recorded_before["contract"], recorded_after["contract"])
        self.assertNotEqual(before["generator"], recorded_after["generator"])
        with self.assertRaisesRegex(ValueError, "mobile binding contract mismatch"):
            mobile.preflight(self.args.out, self.root / "mobile", "ios")
        self.config.write_text(original)
        artifacts.render(self.args)
        mobile.preflight(self.args.out, self.root / "mobile", "ios")

    def test_both_receipt_producers_pass_mobile_preflight(self):
        artifacts.build(self.args)
        artifacts.render(self.args)
        self.native_receipts()
        mobile.preflight(self.args.out, self.root / "mobile", "ios")
        binaries = {
            role: Path(next(iter(item["files"])))
            for role, item in json.loads(
                (self.args.artifacts / "artifacts.json").read_text()
            )["artifacts"].items()
        }
        receipt.record(self.args.out, binaries)
        mobile.preflight(self.args.out, self.root / "mobile", "ios")
        recorded = json.loads((self.args.out / "swift/sdk-contract.json").read_text())
        self.assertEqual(recorded["artifact"]["source"], artifacts.source_hash())

    def test_conformance_bindings_rejected_before_mobile_assembly(self):
        self.args.features = "conformance"
        artifacts.build(self.args)
        artifacts.render(self.args)
        self.native_receipts()
        with self.assertRaisesRegex(ValueError, "mobile binding feature mismatch"):
            mobile.preflight(self.args.out, self.root / "mobile", "ios")
        self.assertFalse((self.root / "products").exists())
        self.args.features = ""
        artifacts.build(self.args)
        artifacts.render(self.args)
        mobile.preflight(self.args.out, self.root / "mobile", "ios")

    def test_android_build_selects_all_four_ndk_target_tools(self):
        tools = self.root / "ndk/toolchains/llvm/prebuilt/linux-x86_64/bin"
        tools.mkdir(parents=True)
        (tools / "llvm-ar").write_text("archiver")
        for triple in mobile.ANDROID.values():
            target = (
                "armv7a-linux-androideabi"
                if triple == "armv7-linux-androideabi"
                else triple
            )
            for suffix in ("23-clang", "23-clang++"):
                (tools / (target + suffix)).write_text("compiler")
        calls = []
        with (
            patch.dict(os.environ, {"ANDROID_NDK_HOME": str(self.root / "ndk")}),
            patch.object(mobile.sys, "platform", "linux"),
            patch.object(
                mobile.sys,
                "argv",
                [
                    "mobile-package.py",
                    "build",
                    "android",
                    "--artifacts",
                    str(self.root / "android"),
                ],
            ),
            patch.object(
                mobile,
                "run",
                side_effect=lambda command, **kw: calls.append((command, kw)),
            ),
        ):
            mobile.main()
        self.assertEqual(len(calls), 4)
        self.assertEqual(
            {command[command.index("--rust-target") + 1] for command, _ in calls},
            set(mobile.ANDROID.values()),
        )
        for command, kwargs in calls:
            triple = command[command.index("--rust-target") + 1]
            target = triple.replace("-", "_")
            env = kwargs["env"]
            linker = env["CARGO_TARGET_" + target.upper() + "_LINKER"]
            self.assertEqual(linker, env["CC_" + target])
            self.assertTrue(linker.endswith("23-clang"))
            self.assertTrue(env["CXX_" + target].endswith("23-clang++"))
            self.assertEqual(env["AR_" + target], str(tools / "llvm-ar"))
            self.assertEqual(command[-1], str(self.root / "android" / triple))


if __name__ == "__main__":
    unittest.main()
