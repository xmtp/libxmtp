"""Run bounded controls through the benchmark's actual callers."""

import copy
import hashlib
import json
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from pathlib import Path
from unittest.mock import patch

import baselines
from fixtures import dataset, digest, expected_observation

sys.path.insert(0, str(Path(__file__).parent / "hosts"))
import processes
from driver import record_observation
from runner import summarize
from test_gate import ledger

SOURCE = Path(__file__).resolve().parent
COMMIT = "a" * 40


def resolver_inputs():
    data = {}
    for target in ("node", "browser"):
        archive = f"https://fixture/{target}.tgz"
        body = target.encode()
        integrity = (
            "sha512-"
            + baselines.base64.b64encode(hashlib.sha512(body).digest()).decode()
        )
        data[archive] = body
        data[f"https://registry.npmjs.org/@xmtp%2f{target}-sdk"] = json.dumps(
            {
                "time": {"1.0.0": "2026-01-01"},
                "versions": {
                    "1.0.0": {
                        "gitHead": COMMIT,
                        "dist": {"tarball": archive, "integrity": integrity},
                    }
                },
            }
        ).encode()
    base = "https://repo.maven.apache.org/maven2/org/xmtp/android/1.0.0/android-1.0.0"
    data["https://repo.maven.apache.org/maven2/org/xmtp/android/maven-metadata.xml"] = (
        b"<metadata><versioning><versions><version>1.0.0</version></versions></versioning></metadata>"
    )
    data[base + ".pom"] = (
        b'<project xmlns="http://maven.apache.org/POM/4.0.0"><groupId>org.xmtp</groupId><artifactId>android</artifactId><version>1.0.0</version></project>'
    )
    data[base + ".aar"] = b"actual AAR bytes"
    data[base + ".aar.sha256"] = (
        hashlib.sha256(data[base + ".aar"]).hexdigest().encode()
    )
    api = "https://api.github.com/repos/xmtp/libxmtp/"
    ref = {
        "ref": "refs/tags/android-1.0.0",
        "object": {"type": "commit", "sha": COMMIT},
    }
    data[api + "git/ref/tags/android-1.0.0"] = json.dumps(ref).encode()
    source = f"https://raw.githubusercontent.com/xmtp/libxmtp/{COMMIT}/"
    data[source + "sdks/android/gradle.properties"] = b"version=1.0.0\n"
    ref["ref"] = "refs/tags/ios-1.0.0"
    data[api + "git/matching-refs/tags/ios-"] = json.dumps([ref]).encode()
    data[api + "releases/tags/ios-1.0.0"] = json.dumps(
        {
            "draft": False,
            "prerelease": False,
            "published_at": "2026-01-01T00:00:00Z",
            "url": api + "releases/1",
        }
    ).encode()
    manifest = []
    for name in ("Static", "Dynamic"):
        url = f"https://github.com/xmtp/libxmtp/releases/download/fixture/{name}.zip"
        data[url] = name.encode()
        manifest.append(
            f'url: "{url}", checksum: "{hashlib.sha256(data[url]).hexdigest()}"'
        )
    data[source + "Package.swift"] = "\n".join(manifest).encode()
    return data


class ResolverControls(unittest.TestCase):
    def resolve(self, inputs, output):
        headers = {"Last-Modified": "Thu, 01 Jan 2026 00:00:00 GMT"}
        with (
            patch.object(baselines, "fetch", side_effect=lambda url: inputs[url]),
            patch.object(
                baselines,
                "fetch_response",
                side_effect=lambda url: (inputs[url], headers),
            ),
        ):
            output.write_text(json.dumps(baselines.resolve()))
        return json.loads(output.read_text())

    def test_archive_bytes_before_lock(self):
        original = resolver_inputs()
        with tempfile.TemporaryDirectory() as temp:
            output = Path(temp) / "lock.json"
            value = self.resolve(original, output)
            self.assertEqual(value["packages"]["kotlin"]["commit"], COMMIT)
            self.assertIn("publication_evidence", value["packages"]["kotlin"])
            for url in [key for key in original if key.endswith((".aar", ".zip"))]:
                inputs = dict(original)
                inputs[url] += b"tamper"
                output.unlink(missing_ok=True)
                with self.assertRaisesRegex(ValueError, "checksum mismatch"):
                    self.resolve(inputs, output)
                self.assertFalse(output.exists())

    def test_missing_or_conflicting_android_source(self):
        for fault in ("source", "tag", "pom", "date"):
            inputs = resolver_inputs()
            source = f"https://raw.githubusercontent.com/xmtp/libxmtp/{COMMIT}/sdks/android/gradle.properties"
            if fault == "source":
                inputs[source] = b"version=2.0.0\n"
            if fault == "tag":
                url = "https://api.github.com/repos/xmtp/libxmtp/git/ref/tags/android-1.0.0"
                value = json.loads(inputs[url])
                value["ref"] = "refs/tags/android-2.0.0"
                inputs[url] = json.dumps(value).encode()
            if fault == "pom":
                url = next(key for key in inputs if key.endswith(".pom"))
                inputs[url] = inputs[url].replace(b"1.0.0", b"2.0.0")
            with tempfile.TemporaryDirectory() as temp:
                output = Path(temp) / "lock.json"
                if fault == "date":
                    with (
                        patch.object(
                            baselines, "fetch", side_effect=lambda url: inputs[url]
                        ),
                        patch.object(
                            baselines,
                            "fetch_response",
                            side_effect=lambda url: (inputs[url], {}),
                        ),
                    ):
                        with self.assertRaisesRegex(ValueError, "date evidence"):
                            baselines.resolve()
                else:
                    with self.assertRaises(ValueError):
                        self.resolve(inputs, output)
                self.assertFalse(output.exists())


class DriverControls(unittest.TestCase):
    def test_observed_page_order(self):
        fixture = dataset()
        with tempfile.TemporaryDirectory() as temp:
            for fault in ("good", "reverse", "swap", "content"):
                values = copy.deepcopy(fixture["messages"][:1000])
                if fault == "reverse":
                    values.reverse()
                if fault == "swap":
                    values[0], values[1] = values[1], values[0]
                if fault == "content":
                    values[0]["text"] = "wrong"
                response = {"observed_messages": values}
                log = Path(temp) / fault
                record_observation(response, fixture, "page", log)
                self.assertEqual(
                    json.loads(log.with_suffix(".observations.json").read_text()),
                    values,
                )
                self.assertEqual(
                    response["observation"] == expected_observation(fixture, "page"),
                    fault == "good",
                )

    def test_actual_driver_safety_evidence(self):
        fixture = dataset()
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / "fixture.json").write_text(json.dumps(fixture))
            host = root / "host.py"
            host.write_text(
                "import json,sys\nr=json.load(sys.stdin)\nv=json.load(open(sys.argv[1]))\nv['source']={k:r[k] for k in ('fixture_sha256','package_sha256')}\nprint(json.dumps(v))\n"
            )
            result_file = root / "result.json"
            config = root / "config.json"
            config.write_text(
                json.dumps(
                    {"host_command": [sys.executable, str(host), str(result_file)]}
                )
            )
            for target in ("node", "browser", "swift", "kotlin"):
                for flags in (
                    None,
                    {
                        "correctness": False,
                        "deadlock": True,
                        "use_after_end": True,
                        "retained_growth": True,
                    },
                ):
                    result = {
                        "completed": True,
                        "duration_ms": 1,
                        "peak_memory_bytes": 1,
                        "memory_scope": "ios-app-resident-high-water",
                    }
                    if flags is not None:
                        result["safety"] = flags
                    result_file.write_text(json.dumps(result))
                    request = {
                        "phase": "measure",
                        "target": target,
                        "workload": "cold_start",
                        "state_directory": str(root),
                        "fixture_sha256": digest(fixture),
                        "package_sha256": "fixture",
                    }
                    child = subprocess.run(
                        [sys.executable, str(SOURCE / "hosts/driver.py"), str(config)],
                        input=json.dumps(request),
                        text=True,
                        capture_output=True,
                        check=True,
                    )
                    response = json.loads(child.stdout)
                    self.assertEqual(
                        response["safety"],
                        flags
                        or {
                            key: None
                            for key in (
                                "correctness",
                                "deadlock",
                                "use_after_end",
                                "retained_growth",
                            )
                        },
                    )

    def test_null_safety_is_pending(self):
        value = ledger()
        for row in value["samples"]:
            row["response"]["safety"] = dict.fromkeys(row["response"]["safety"])
        report = summarize(value)
        self.assertEqual(report["performance_decision"], "PASS")
        self.assertEqual(report["safety_decision"], "PENDING")
        self.assertEqual(report["safety_failures"], [])


class SamplerControls(unittest.TestCase):
    def test_delayed_thread_start_short_child(self):
        original = threading.Thread

        def delayed(*args, **kwargs):
            target = kwargs["target"]

            def held():
                time.sleep(0.2)
                target()

            kwargs["target"] = held
            return original(*args, **kwargs)

        with patch.object(processes.threading, "Thread", side_effect=delayed):
            result = processes.execute(
                [sys.executable, "-c", "import sys; sys.stdin.read(); print('short')"],
                "go",
            )
        self.assertEqual(result[0], 0)
        self.assertEqual(result[1].strip(), "short")
        self.assertGreater(result[4], 0)

    def test_sampling_error_and_child_failure(self):
        with patch.object(
            processes.subprocess, "run", side_effect=OSError("ps unavailable")
        ):
            with self.assertRaises(OSError):
                processes.execute(
                    [sys.executable, "-c", "import sys; sys.stdin.read()"], "go"
                )
        code, _, _, _, peak = processes.execute(
            [sys.executable, "-c", "import sys; sys.stdin.read(); sys.exit(7)"], "go"
        )
        self.assertEqual(code, 7)
        self.assertGreater(peak, 0)


if __name__ == "__main__":
    unittest.main()
