#!/usr/bin/env python3
"""Exercise the exact retained Maven consumer gate without an emulator."""

import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
import xml.etree.ElementTree as ET

import yaml

ROOT = Path(__file__).resolve().parents[2]
SDK = ROOT / "sdks/android"
WORKFLOW = ROOT / ".github/workflows/release-android.yml"
VERSION = "8.0.0-rc1"


class AndroidReleaseConsumerTest(unittest.TestCase):
    def test_release_gates_sealing_on_retained_consumer_and_keeps_minimum_sdk(self):
        steps = yaml.safe_load(WORKFLOW.read_text())["jobs"]["build"]["steps"]
        consumer = next(
            step for step in steps if "test-release-repository" in step.get("run", "")
        )
        minimum = next(step for step in steps if "test-min-sdk" in step.get("run", ""))
        seal = next(
            step for step in steps if "sdk_version.py seal" in step.get("run", "")
        )
        for step in (consumer, minimum):
            self.assertNotIn("if", step)
            self.assertNotIn("continue-on-error", step)
            self.assertLess(steps.index(step), steps.index(seal))
            self.assertIn("-no-window", step["env"]["NIX_ANDROID_EMULATOR_FLAGS"])
            self.assertIn("-memory 3072", step["env"]["NIX_ANDROID_EMULATOR_FLAGS"])
        self.assertEqual(consumer["env"]["VERSION"], "${{ inputs.version }}")
        self.assertIn('test-release-repository "$VERSION"', consumer["run"])
        kvm = next(step for step in steps if step.get("name") == "Enable KVM")
        self.assertLess(steps.index(kvm), steps.index(consumer))
        self.assertIn("udevadm trigger --name-match=kvm", kvm["run"])
        recipe = (
            (SDK / "android.just")
            .read_text()
            .split("test-release-repository version:\n")[1]
            .split("\n\n")[0]
        )
        self.assertIn("NIX_ANDROID_EMULATOR_API=23 run-test-emulator --", recipe)
        self.assertIn("check-release-consumer.py", recipe)
        self.assertIn('target/release/android/repository" "{{ version }}"', recipe)

    def test_native_package_check_uses_retained_aar_and_propagates_failure(self):
        steps = yaml.safe_load(WORKFLOW.read_text())["jobs"]["build"]["steps"]
        prepare = next(
            step
            for step in steps
            if step.get("name") == "Prepare the Maven publication without uploading"
        )
        self.assertEqual(prepare["env"]["VERSION"], "${{ inputs.version }}")
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            (root / "dev").mkdir()
            (root / "sdks/android/dev").mkdir(parents=True)
            wrapper = root / "dev/nix-shell"
            wrapper.write_text('#!/bin/bash\nexec bash -euc "$1"\n')
            wrapper.chmod(0o755)
            gradle = root / "sdks/android/gradlew"
            gradle.write_text("#!/bin/sh\nexit 0\n")
            gradle.chmod(0o755)
            checker = root / "sdks/android/dev/check-native-packages.py"
            checker.write_text(
                "import json, os, sys\n"
                "open(os.environ['CHECK_LOG'], 'w').write(json.dumps(sys.argv[1:]))\n"
                "sys.exit(int(os.environ['CHECK_EXIT']))\n"
            )
            log = root / "check.json"
            for status in (0, 42):
                result = subprocess.run(
                    ["bash", "-euc", prepare["run"]],
                    cwd=root,
                    env=dict(
                        os.environ,
                        VERSION=VERSION,
                        CHECK_LOG=str(log),
                        CHECK_EXIT=str(status),
                    ),
                    capture_output=True,
                    text=True,
                )
                self.assertEqual(result.returncode, status, result.stderr)
                self.assertEqual(
                    json.loads(log.read_text()),
                    [
                        "--require-all-abis",
                        f"target/release/android/repository/org/xmtp/android/{VERSION}/android-{VERSION}.aar",
                    ],
                )

    def make_sdk(self, root):
        sdk = root / "sdks/android"
        shutil.copytree(SDK / "dev/release-consumer", sdk / "dev/release-consumer")
        shutil.copyfile(
            SDK / "dev/check-release-consumer.py", sdk / "dev/check-release-consumer.py"
        )
        shutil.copytree(SDK / "gradle", sdk / "gradle")
        shutil.copyfile(SDK / "gradle.properties", sdk / "gradle.properties")
        wrapper = sdk / "gradlew"
        wrapper.write_text(
            f"#!{sys.executable}\n"
            "import json, os, pathlib, sys\n"
            "args = sys.argv[1:]\n"
            "project = pathlib.Path(args[args.index('-p') + 1])\n"
            "with open(os.environ['GRADLE_LOG'], 'a') as log:\n"
            " log.write(json.dumps({'args': args, 'inventory': (project / 'gradle/verification-metadata.xml').read_text(), 'lock': (project / 'gradle.lockfile').read_text()}) + '\\n')\n"
            "reports = project / 'build/outputs/androidTest-results/connected/fixture'\n"
            "reports.mkdir(parents=True)\n"
            "(reports / 'TEST-consumer.xml').write_text('<testsuite>fixture instrumentation report</testsuite>')\n"
            "sys.exit(42 if os.environ.get('FAIL_METADATA') in args else 0)\n"
        )
        wrapper.chmod(0o755)
        repository = root / "retained repository"
        publication = repository / "org/xmtp/android" / VERSION
        publication.mkdir(parents=True)
        for ext in ("aar", "pom", "module"):
            (publication / f"android-{VERSION}.{ext}").write_text(f"retained {ext}")
        return sdk, repository, publication

    def invoke_consumer(self, root, sdk, repository, *args, **env):
        return subprocess.run(
            [
                sys.executable,
                str(sdk / "dev/check-release-consumer.py"),
                str(repository),
                VERSION,
                *args,
            ],
            env=dict(os.environ, GRADLE_LOG=str(root / "gradle.jsonl"), **env),
            capture_output=True,
            text=True,
        )

    def test_consumer_installs_both_metadata_formats_with_frozen_checksums(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            sdk, repository, publication = self.make_sdk(root)
            for args in ((), ("--build-only",)):
                log = root / "gradle.jsonl"
                log.unlink(missing_ok=True)
                result = self.invoke_consumer(root, sdk, repository, *args)
                self.assertEqual(result.returncode, 0, result.stderr)
                calls = [json.loads(line) for line in log.read_text().splitlines()]
                self.assertEqual(len(calls), 2)
                projects = set()
                for metadata, call in zip(("module", "pom"), calls, strict=True):
                    argv = call["args"]
                    projects.add(argv[argv.index("-p") + 1])
                    self.assertIn(f"-PxmtpMetadata={metadata}", argv)
                    self.assertIn(f"-PxmtpRepository={repository}", argv)
                    self.assertIn(f"-PxmtpVersion={VERSION}", argv)
                    self.assertIn("assembleDebug", argv)
                    self.assertIn("assembleDebugAndroidTest", argv)
                    self.assertEqual("connectedDebugAndroidTest" in argv, not args)
                    self.assertIn("--dependency-verification=strict", argv)
                    self.assertIn("net.java.dev.jna:jna:5.18.1=", call["lock"])
                    self.assertIn(
                        "org.jetbrains.kotlinx:kotlinx-coroutines-core:1.8.0=",
                        call["lock"],
                    )
                    self.assertIn("\nempty=", call["lock"])
                    self.assertIn("androidx.test:runner:1.5.2=", call["lock"])
                    self.assertIn(
                        "org.jetbrains.kotlin:kotlin-compiler-embeddable:2.2.20=",
                        call["lock"],
                    )
                    self.assertNotIn("org.xmtp:android:", call["lock"])
                    inventory = ET.fromstring(call["inventory"])
                    namespace = {
                        "v": "https://schema.gradle.org/dependency-verification"
                    }
                    component = inventory.find(
                        f"v:components/v:component[@group='org.xmtp'][@name='android'][@version='{VERSION}']",
                        namespace,
                    )
                    self.assertIsNotNone(component)
                    self.assertEqual(len(component), 3)
                    for artifact in component:
                        expected = hashlib.sha256(
                            (publication / artifact.attrib["name"]).read_bytes()
                        ).hexdigest()
                        self.assertEqual(artifact[0].attrib["value"], expected)
                self.assertEqual(len(projects), 2)

    def test_missing_metadata_and_gradle_failures_block_the_gate(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            sdk, repository, publication = self.make_sdk(root)
            log = root / "gradle.jsonl"
            for ext in ("aar", "pom", "module"):
                artifact = publication / f"android-{VERSION}.{ext}"
                saved = artifact.read_bytes()
                artifact.unlink()
                result = self.invoke_consumer(root, sdk, repository)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("Missing retained publication file", result.stderr)
                self.assertFalse(log.exists())
                artifact.write_bytes(saved)
            for metadata, count in (("module", 1), ("pom", 2)):
                log.unlink(missing_ok=True)
                result = self.invoke_consumer(
                    root, sdk, repository, FAIL_METADATA=f"-PxmtpMetadata={metadata}"
                )
                self.assertEqual(result.returncode, 42, result.stderr)
                self.assertIn("fixture instrumentation report", result.stdout)
                self.assertEqual(len(log.read_text().splitlines()), count)

    def test_fixture_uses_metadata_not_project_or_flat_files(self):
        fixture = SDK / "dev/release-consumer"
        settings = (fixture / "settings.gradle").read_text()
        build = (fixture / "build.gradle").read_text()
        runtime = (
            fixture
            / "src/androidTest/java/org/xmtp/releaseconsumer/RetainedPublicationTest.kt"
        ).read_text()
        self.assertIn("exclusiveContent", settings)
        self.assertIn("includeModule('org.xmtp', 'android')", settings)
        self.assertIn("mavenPom()", settings)
        self.assertIn("ignoreGradleMetadataRedirection()", settings)
        self.assertIn("gradleMetadata()", settings)
        self.assertNotRegex(
            settings + build,
            r"includeBuild|project\(':library'\)|flatDir|implementation files",
        )
        self.assertIn('implementation "org.xmtp:android:${frozenVersion}"', build)
        self.assertIn("it.moduleVersion.id.name == 'android'", build)
        self.assertIn("activateDependencyLocking()", build)
        self.assertIn("LockMode.STRICT", build)
        self.assertIn("lockAllConfigurations()", build)
        self.assertEqual(build.count("ignoredDependencies.add("), 1)
        self.assertIn("ignoredDependencies.add('org.xmtp:android')", build)
        self.assertTrue((fixture / "buildscript-gradle.lockfile").is_file())
        self.assertTrue((fixture / "gradle.lockfile").is_file())
        self.assertIn("androidTestImplementation 'androidx.test:runner:1.5.2'", build)
        self.assertIn("digest(sdk.first().file) != digest(retained)", build)
        self.assertIn("dependsOn verifyRetainedArtifact", build)
        self.assertIn('\'EXPECTED_VERSION\', "\\"${frozenVersion}\\""', build)
        self.assertIn(
            "assertEquals(BuildConfig.EXPECTED_VERSION, sdkVersion())", runtime
        )
        self.assertIn("Date.from(instant)", runtime)


if __name__ == "__main__":
    unittest.main()
