"""Check that incomplete or slow device results cannot satisfy the gate."""

import copy
import json
import subprocess
import unittest
from pathlib import Path
import tempfile
import xml.etree.ElementTree as ET
from unittest.mock import MagicMock, patch
import run
from run import validate


def result():
    return {
        "api": 34,
        "abi": "x86_64",
        "cores": 4,
        "hardware": "ranchu",
        "memoryKb": 4_000_000,
        "groups": 1000,
        "messages": "100000",
        "heavyMessages": 50000,
        "warmups": 5,
        "measuredRuns": 30,
        "bodyAsciiBytes": 256,
        "visitedTranscripts": 10,
        "maxCacheTranscripts": 3,
        "sdkVersion": "8.0.0",
        "seedMs": 1000,
        "workloadId": "fixture-1",
        "startupReplayRows": 0,
        "replayRowsAfterMeasurements": 0,
        "firstMs": [100.0] * 30,
        "olderMs": [100.0] * 30,
        "listMs": [100.0] * 30,
        "heapDeltaBytes": 1024,
        "maxTranscriptRows": 500,
        "maxCacheRows": 1500,
        "maxPageRows": 500,
        "maxHistoryReadRows": 50,
    }


class PerformanceGateTest(unittest.TestCase):
    def test_complete_fixed_result_passes(self):
        validate(result())

    def test_rejects_other_device_and_short_workload(self):
        for key, value in (
            ("api", 35),
            ("abi", "arm64-v8a"),
            ("cores", 8),
            ("hardware", "physical"),
            ("memoryKb", 2_000_000),
            ("groups", 999),
            ("messages", "99999"),
            ("heavyMessages", 49999),
            ("warmups", 4),
            ("measuredRuns", 29),
            ("bodyAsciiBytes", 255),
            ("visitedTranscripts", 9),
            ("maxCacheTranscripts", 4),
            ("sdkVersion", ""),
            ("seedMs", 0),
            ("workloadId", ""),
            ("startupReplayRows", 2),
            ("replayRowsAfterMeasurements", 1),
        ):
            with self.subTest(key=key), self.assertRaises(ValueError):
                report = result()
                report[key] = value
                validate(report)

    def test_uses_nearest_rank_p95(self):
        report = result()
        report["firstMs"] = [300.0] * 29 + [1000.0]
        validate(report)
        report["firstMs"][-2] = 301.0
        with self.assertRaisesRegex(ValueError, "firstMs"):
            validate(report)

    def test_rejects_missing_short_invalid_or_slow_measurements(self):
        for metric, limit in (("firstMs", 300), ("olderMs", 250), ("listMs", 1000)):
            for samples in (
                [],
                [1.0] * 29,
                [float("nan")] * 30,
                [-1.0] * 30,
                [limit + 1.0] * 30,
            ):
                with (
                    self.subTest(metric=metric, samples=samples),
                    self.assertRaises(ValueError),
                ):
                    report = result()
                    report[metric] = samples
                    validate(report)

    def test_rejects_removed_row_or_cache_bound_and_heap_overflow(self):
        for metric, value in (
            ("maxTranscriptRows", 501),
            ("maxCacheRows", 1501),
            ("maxPageRows", 501),
            ("maxHistoryReadRows", 51),
            ("heapDeltaBytes", 64 * 1024 * 1024 + 1),
            ("maxCacheRows", 0),
            ("maxTranscriptRows", 0),
            ("heapDeltaBytes", -1),
        ):
            with (
                self.subTest(metric=metric),
                self.assertRaisesRegex(ValueError, metric),
            ):
                report = copy.deepcopy(result())
                report[metric] = value
                validate(report)


class DeviceInvocationTest(unittest.TestCase):
    def test_later_unavailable_or_different_seed_reads_keep_first_workload_evidence(
        self,
    ):
        original = '{"phase":"group-tail","group":999,"status":"complete"}\n'
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            identity = {"workloadId": "fixture-1", "seedMs": 1000}
            progress = subprocess.CompletedProcess([], 0, stdout=original)
            run.seed_evidence(output, "green", identity, progress, False)
            unavailable = subprocess.CompletedProcess(
                [], 1, stdout="remote read failed"
            )
            run.seed_evidence(output, "red-cache", identity, unavailable, True)
            self.assertEqual(
                original,
                (output / "workload-seed-progress.jsonl").read_text(),
                "Later failed read erased seed evidence",
            )
            metadata = json.loads((output / "red-cache-workload.json").read_text())
            self.assertEqual("unavailable", metadata["seedReadStatus"])
            self.assertEqual("fixture-1", metadata["retainedWorkloadId"])
            cases = [
                (
                    identity,
                    subprocess.CompletedProcess([], 0, stdout="different seed\n"),
                ),
                ({"workloadId": "other-fixture", "seedMs": 1000}, progress),
            ]
            for report, read in cases:
                with self.subTest(report=report, stdout=read.stdout):
                    with self.assertRaisesRegex(
                        ValueError, "differs from the retained workload"
                    ):
                        run.seed_evidence(output, "restored", report, read, True)
                    self.assertEqual(
                        original, (output / "workload-seed-progress.jsonl").read_text()
                    )
                    self.assertEqual(
                        "different",
                        json.loads((output / "restored-workload.json").read_text())[
                            "seedReadStatus"
                        ],
                    )

    def test_failed_device_run_retains_seed_readiness_and_measurement_progress(self):
        progress = '{"group":0,"expectedPublished":256,"status":"failed"}\n'
        readiness = '{"phase":"restored-owner","event":"failed","failureClass":"TimeoutCancellationException"}\n'
        measurement = (
            '{"run":5,"olderMs":468.4,"olderSdkMs":465.0,"olderMappingMs":3.0}\n'
        )

        def execute_process(arguments, **options):
            if arguments[0] != "adb":
                return subprocess.CompletedProcess(arguments, 1)
            if arguments[-1] == "files/messenger-performance/seed-progress.jsonl":
                return subprocess.CompletedProcess(arguments, 0, stdout=progress)
            if arguments[-1] == "files/messenger-performance/readiness-progress.jsonl":
                return subprocess.CompletedProcess(arguments, 0, stdout=readiness)
            if (
                arguments[-1]
                == "files/messenger-performance/measurement-progress.jsonl"
                and "cat" in arguments
            ):
                return subprocess.CompletedProcess(arguments, 0, stdout=measurement)
            return subprocess.CompletedProcess(arguments, 1, stdout="")

        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            with (
                patch.dict(run.os.environ, {"ANDROID_SERIAL": "emulator-5560"}),
                patch.object(run, "ANDROID", output / "android"),
                patch.object(run.subprocess, "run", side_effect=execute_process),
                patch.object(
                    run.subprocess, "Popen", return_value=MagicMock(stdout=iter(()))
                ),
                patch.object(run, "result_failures", return_value=["receipt barrier"]),
            ):
                code, report, failures = run.execute(output, "green", "http://fixture")
            self.assertEqual(1, code)
            self.assertEqual({}, report)
            self.assertEqual(["receipt barrier"], failures)
            self.assertEqual(
                progress, (output / "workload-seed-progress.jsonl").read_text()
            )
            self.assertEqual(
                readiness, (output / "green-readiness-progress.jsonl").read_text()
            )
            self.assertEqual(
                measurement, (output / "green-measurement-progress.jsonl").read_text()
            )

    def test_one_workload_seed_keeps_identity_and_reuse_attribution_across_passes(self):
        progress = '{"phase":"group-tail","group":999,"status":"complete"}\n'
        label = "green"

        def process(arguments, **options):
            if arguments[0] != "adb":
                return subprocess.CompletedProcess(arguments, 0)
            if arguments[-1] == "files/messenger-performance/workload.json":
                return subprocess.CompletedProcess(
                    arguments, int(label == "green"), stdout=""
                )
            if "cat" in arguments:
                data = (
                    progress
                    if arguments[-1].endswith("seed-progress.jsonl")
                    else json.dumps(result())
                    if arguments[-1].endswith("result.json")
                    else ""
                )
                return subprocess.CompletedProcess(arguments, 0, stdout=data)
            return subprocess.CompletedProcess(arguments, 0, stdout="")

        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            with (
                patch.dict(run.os.environ, {"ANDROID_SERIAL": "emulator-5560"}),
                patch.object(run, "ANDROID", output / "android"),
                patch.object(run.subprocess, "run", side_effect=process),
                patch.object(
                    run.subprocess, "Popen", return_value=MagicMock(stdout=iter(()))
                ),
                patch.object(run, "result_failures", return_value=[]),
            ):
                for label in ("green", "red-cache", "restored"):
                    (output / f"{label}-seed-progress.jsonl").write_text(
                        "old labeled seed"
                    )
                    run.execute(output, label, "http://fixture")
                    self.assertFalse(
                        (output / f"{label}-seed-progress.jsonl").exists(),
                        "Reused seed was published as per-pass progress",
                    )
                    self.assertEqual(
                        progress, (output / "workload-seed-progress.jsonl").read_text()
                    )
                    identity = json.loads(
                        (output / f"{label}-workload.json").read_text()
                    )
                    self.assertEqual("fixture-1", identity["workloadId"])
                    self.assertEqual(1000, identity["seedMs"])
                    self.assertEqual(
                        label != "green", identity["manifestPresentBeforeInvocation"]
                    )
                    if label != "green":
                        self.assertEqual(
                            "reused existing seed",
                            identity["seedAttribution"],
                            "Retained workload checkpoints were marked as new",
                        )
                self.assertEqual(1, len(list(output.glob("*seed-progress.jsonl"))))

    def test_failed_invocation_cannot_republish_previous_readiness(self):
        stale = '{"phase":"restored-owner","event":"complete"}\n'
        state = {"files/messenger-performance/readiness-progress.jsonl": stale}

        def process(arguments, **options):
            if arguments[0] != "adb":
                return subprocess.CompletedProcess(arguments, 1)
            if "rm" in arguments:
                for name in arguments:
                    state.pop(name, None)
            return subprocess.CompletedProcess(
                arguments,
                0 if arguments[-1] in state else 1,
                stdout=state.get(arguments[-1], ""),
            )

        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            with (
                patch.dict(run.os.environ, {"ANDROID_SERIAL": "emulator-5560"}),
                patch.object(run, "ANDROID", output / "android"),
                patch.object(run.subprocess, "run", side_effect=process),
                patch.object(
                    run.subprocess, "Popen", return_value=MagicMock(stdout=iter(()))
                ),
                patch.object(run, "result_failures", return_value=["setup failed"]),
            ):
                code, _, _ = run.execute(output, "restored", "http://fixture")
            self.assertEqual(1, code)
            self.assertEqual(
                "",
                (output / "restored-readiness-progress.jsonl").read_text(),
                "Previous readiness was republished after setup failure",
            )

    def test_every_pass_retains_the_installed_app_for_result_and_dataset_reuse(self):
        invocations = []

        def execute_process(arguments, **options):
            if arguments[0] != "adb":
                invocations.append(arguments)
            return subprocess.CompletedProcess(
                arguments, 0, stdout=json.dumps(result())
            )

        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            logcat = MagicMock(stdout=iter(()))
            with (
                patch.dict(run.os.environ, {"ANDROID_SERIAL": "emulator-5560"}),
                patch.object(run, "ANDROID", output / "android"),
                patch.object(run.subprocess, "run", side_effect=execute_process),
                patch.object(run.subprocess, "Popen", return_value=logcat),
                patch.object(run, "result_failures", return_value=[]),
            ):
                for label in ("green", "red-cache", "restored"):
                    code, report, failures = run.execute(
                        output, label, "http://fixture"
                    )
                    self.assertEqual(0, code)
                    self.assertEqual("fixture-1", report["workloadId"])
                    self.assertEqual([], failures)

            self.assertEqual(3, len(invocations))
            for invocation in invocations:
                self.assertIn(":example:connectedDebugAndroidTest", invocation)
                self.assertIn(
                    "-Pandroid.injected.androidTest.leaveApksInstalledAfterRun=true",
                    invocation,
                    "UTP otherwise removes the app, result, database and Keystore records",
                )


class CacheFailureControlTest(unittest.TestCase):
    def test_stale_named_xml_cannot_approve_an_unrelated_current_failure(self):
        actual = (
            run.ANDROID
            / "example/src/main/java/org/xmtp/android/example/messenger/SDKHistoryPages.kt"
        ).read_text()
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            android = output / "android"
            source = (
                android
                / "example/src/main/java/org/xmtp/android/example/messenger/SDKHistoryPages.kt"
            )
            source.parent.mkdir(parents=True)
            source.write_text(actual)
            connected = android / "example/build/outputs/androidTest-results/connected"
            snapshot = output / "red-cache-connected"

            def xml(folder, name, failure):
                folder.mkdir(parents=True, exist_ok=True)
                suite = ET.Element("testsuite")
                case = ET.SubElement(suite, "testcase", classname=run.TEST_CLASS)
                ET.SubElement(case, "failure").text = failure
                ET.ElementTree(suite).write(folder / name)

            xml(connected, "stale.xml", "Transcript cache trimming was removed")
            xml(snapshot, "stale.xml", "Transcript cache trimming was removed")
            red = result()
            red.update(maxCacheRows=5000, maxCacheTranscripts=10)
            invocations = []

            def process(arguments, **options):
                if arguments[0] != "adb":
                    invocations.append(arguments)
                    if len(invocations) > 1:
                        self.fail(
                            "Stale XML incorrectly advanced the control to restored"
                        )
                    xml(connected, "current.xml", "Unrelated current failure")
                    return subprocess.CompletedProcess(arguments, 1)
                if arguments[-1] == "files/messenger-performance/result.json":
                    return subprocess.CompletedProcess(
                        arguments, 0, stdout=json.dumps(red)
                    )
                return subprocess.CompletedProcess(arguments, 1, stdout="")

            with (
                patch.dict(run.os.environ, {"ANDROID_SERIAL": "emulator-5560"}),
                patch.object(run, "ANDROID", android),
                patch.object(run.subprocess, "run", side_effect=process),
                patch.object(
                    run.subprocess, "Popen", return_value=MagicMock(stdout=iter(()))
                ),
            ):
                with self.assertRaisesRegex(ValueError, "broken production cache"):
                    run.cache_red_control(output, "http://fixture", result())
            self.assertEqual(1, len(invocations))
            self.assertEqual(actual, source.read_text())
            self.assertEqual(
                [" Unrelated current failure"], run.result_failures(snapshot)
            )
            self.assertFalse((connected / "stale.xml").exists())
            self.assertFalse((snapshot / "stale.xml").exists())

    def test_missing_or_duplicate_current_performance_xml_fails_closed(self):
        with tempfile.TemporaryDirectory() as directory:
            connected = Path(directory)
            self.assertEqual(
                ["Expected one current performance test XML, found 0"],
                run.result_failures(connected),
            )
            suite = ET.Element("testsuite")
            for _ in range(2):
                case = ET.SubElement(suite, "testcase", classname=run.TEST_CLASS)
                ET.SubElement(
                    case, "failure"
                ).text = "Transcript cache trimming was removed"
            ET.ElementTree(suite).write(connected / "duplicates.xml")
            self.assertEqual(
                ["Expected one current performance test XML, found 2"],
                run.result_failures(connected),
            )

    def test_requires_the_actual_published_cache_class(self):
        actual = (
            run.ANDROID
            / "example/src/main/java/org/xmtp/android/example/messenger/SDKHistoryPages.kt"
        ).read_text()
        unrelated = actual.replace(
            "class SDKTranscriptCache<", "class UnrelatedTranscriptCache<"
        )
        self.assertNotEqual(actual, unrelated)
        with self.assertRaisesRegex(ValueError, "SDKTranscriptCache"):
            run.remove_published_cache_eviction(unrelated)

    def test_restores_production_source_after_interruption(self):
        production = (
            run.ANDROID
            / "example/src/main/java/org/xmtp/android/example/messenger/SDKHistoryPages.kt"
        )
        original = production.read_text()
        with tempfile.TemporaryDirectory() as directory:
            android = Path(directory)
            source = android / production.relative_to(run.ANDROID)
            source.parent.mkdir(parents=True)
            source.write_text(original)
            with (
                patch.object(run, "ANDROID", android),
                patch.object(run, "execute", side_effect=SystemExit(143)),
            ):
                with self.assertRaises(SystemExit):
                    run.cache_red_control(android, "http://fixture", result())
            self.assertEqual(original, source.read_text())

    def test_restores_production_source_and_requires_the_named_failure_on_the_same_dataset(
        self,
    ):
        production = (
            run.ANDROID
            / "example/src/main/java/org/xmtp/android/example/messenger/SDKHistoryPages.kt"
        )
        original = production.read_text()
        for failure, workload_id, restored_id, accepted in (
            ("Transcript cache trimming was removed", "fixture-1", "fixture-1", True),
            ("Unrelated test failed", "fixture-1", "fixture-1", False),
            ("Transcript cache trimming was removed", "fixture-2", "fixture-1", False),
            ("Transcript cache trimming was removed", "fixture-1", "fixture-2", False),
        ):
            with (
                self.subTest(
                    failure=failure, workload=workload_id, restored=restored_id
                ),
                tempfile.TemporaryDirectory() as directory,
            ):
                android = Path(directory)
                source = android / production.relative_to(run.ANDROID)
                source.parent.mkdir(parents=True)
                source.write_text(original)
                red = result()
                red.update(
                    maxCacheRows=5000, maxCacheTranscripts=10, workloadId=workload_id
                )
                restored = result()
                restored["workloadId"] = restored_id

                def execute(output, label, backend):
                    if label == "red-cache":
                        self.assertNotEqual(original, source.read_text())
                        self.assertIn("class SDKTranscriptCache<", source.read_text())
                        self.assertIn("rows.drop(start).take(500)", source.read_text())
                        self.assertNotIn("while (entries.size > 3)", source.read_text())
                        return 1, red, [failure]
                    self.assertEqual(original, source.read_text())
                    return 0, restored, []

                with (
                    patch.object(run, "ANDROID", android),
                    patch.object(run, "execute", execute),
                ):
                    if accepted:
                        run.cache_red_control(android, "http://fixture", result())
                    else:
                        with self.assertRaises(ValueError):
                            run.cache_red_control(android, "http://fixture", result())
                self.assertEqual(original, source.read_text())


if __name__ == "__main__":
    unittest.main()
