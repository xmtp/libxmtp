"""Retained consumers use the reviewed dependency graph and exact SDK bytes."""

import base64
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import threading
import unittest

ROOT = Path(__file__).resolve().parents[2]
VERSION = "8.0.0-rc5"


def archive(manifest, files):
    stream = io.BytesIO()
    with tarfile.open(fileobj=stream, mode="w:gz") as packed:
        for name, content in {"package.json": json.dumps(manifest), **files}.items():
            payload = content.encode()
            info = tarfile.TarInfo("package/" + name)
            info.size = len(payload)
            packed.addfile(info, io.BytesIO(payload))
    return stream.getvalue()


def integrity(payload):
    return "sha512-" + base64.b64encode(hashlib.sha512(payload).digest()).decode()


class InstalledAgentTest(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.directory = Path(temporary.name)
        self.root = self.directory / "repo"
        self.helper = self.root / "dev/release-tools/check-agent-package.mjs"
        self.helper.parent.mkdir(parents=True)
        shutil.copyfile(ROOT / "dev/release-tools/check-agent-package.mjs", self.helper)
        self.write_json(
            "package.json",
            {
                "private": True,
                "packageManager": json.loads((ROOT / "package.json").read_text())[
                    "packageManager"
                ],
            },
        )
        self.workspace = {
            "packages": ["sdks/node", "sdks/agent", "apps/cli"],
            "includeWorkspaceRoot": False,
        }
        self.lock = {
            "lockfileVersion": "9.0",
            "settings": {
                "autoInstallPeers": True,
                "excludeLinksFromLockfile": False,
            },
            "importers": {".": {}, "sdks/node": {}},
        }
        self.manifests = {}
        for sdk, path in (
            ("node-sdk", "sdks/node"),
            ("agent-sdk", "sdks/agent"),
            ("cli", "apps/cli"),
        ):
            manifest = {
                "name": "@xmtp/" + sdk,
                "version": VERSION,
                "type": "module",
                "main": "index.js",
                "scripts": {"prepack": "node -e \"throw Error('Lifecycle ran')\""},
            }
            if sdk != "node-sdk":
                manifest["dependencies"] = {"@xmtp/node-sdk": "workspace:*"}
                self.lock["importers"][path] = {
                    "dependencies": {
                        "@xmtp/node-sdk": {
                            "specifier": "workspace:*",
                            "version": "link:../node"
                            if sdk == "agent-sdk"
                            else "link:../../sdks/node",
                        }
                    }
                }
            self.manifests[sdk] = manifest
            self.write_json(path + "/package.json", manifest)
        self.write_lock()
        self.run_number = 0
        self.registry = "http://127.0.0.1:1/"

    def write_json(self, path, value):
        destination = self.root / path
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text(json.dumps(value))

    def write_lock(self):
        self.write_json("pnpm-workspace.yaml", self.workspace)
        self.write_json("pnpm-lock.yaml", self.lock)

    def pack_sdks(
        self, sdk, pin=VERSION, native=VERSION, export=True, dependency=False
    ):
        consumer = {
            **self.manifests[sdk],
            "dependencies": {"@xmtp/node-sdk": pin},
            "scripts": {"install": "node -e \"throw Error('Lifecycle ran')\""},
        }
        prefix = ""
        if dependency:
            consumer["dependencies"]["viem"] = "^2.55.10"
            prefix = "import {marker} from 'viem'; console.log(marker);\n"
        source = prefix + (
            ("export class Agent {}" if export else "export const missing = true")
            if sdk == "agent-sdk"
            else f"console.log('@xmtp/cli/{VERSION if export else '0.3.0'} darwin node');"
        )
        agent = self.directory / (sdk + ".tgz")
        agent.write_bytes(archive(consumer, {"index.js": source, "bin/run.js": source}))
        node = self.directory / "node.tgz"
        node.write_bytes(
            archive(
                {
                    **self.manifests["node-sdk"],
                    "dependencies": {"@ubjs/core": "0.31.0-5"},
                    "bundledDependencies": ["@ubjs/core"],
                },
                {
                    "index.js": "import {version} from '@ubjs/core'; export function sdkVersion() { return version }",
                    "node_modules/@ubjs/core/package.json": json.dumps(
                        {
                            "name": "@ubjs/core",
                            "version": "0.31.0-5",
                            "type": "module",
                            "main": "index.js",
                        }
                    ),
                    "node_modules/@ubjs/core/index.js": f"export const version = {json.dumps(native)};",
                },
            )
        )
        return agent, node

    def run_helper(self, sdk, agent, node):
        self.run_number += 1
        return subprocess.run(
            ["node", str(self.helper), str(agent), str(node), sdk],
            capture_output=True,
            text=True,
            timeout=120,
            env=dict(
                os.environ,
                TMPDIR=str(self.directory),
                npm_config_registry=self.registry,
                npm_config_cache=str(self.directory / f"npm-cache-{self.run_number}"),
                pnpm_config_registry=self.registry,
                pnpm_config_store_dir=str(
                    self.directory / f"pnpm-store-{self.run_number}"
                ),
                pnpm_config_fetch_retries="0",
                npm_config_update_notifier="false",
            ),
        )

    def test_installed_tarball_must_pin_and_load_the_matching_native_runtime(self):
        for sdk in ("agent-sdk", "cli"):
            for pin, native, export in [
                (VERSION, VERSION, True),
                ("latest", VERSION, True),
                (VERSION, "8.0.0-rc4", True),
                (VERSION, VERSION, False),
            ]:
                with self.subTest(sdk=sdk, pin=pin, native=native, export=export):
                    result = self.run_helper(
                        sdk, *self.pack_sdks(sdk, pin, native, export)
                    )
                    valid = pin == native == VERSION and export
                    self.assertEqual(
                        result.returncode,
                        0 if valid else 1,
                        result.stdout + result.stderr,
                    )
                    if not valid:
                        self.assertIn("AssertionError", result.stderr)

    def test_publisher_pin_keeps_frozen_dependencies_and_inputs_unchanged(self):
        for sdk, path in (("agent-sdk", "sdks/agent"), ("cli", "apps/cli")):
            agent, node = self.pack_sdks(sdk)
            for pin, extra in ((VERSION, False), ("8.0.0-rc4", False), (VERSION, True)):
                with self.subTest(sdk=sdk, pin=pin, extra=extra):
                    manifest = {
                        **self.manifests[sdk],
                        "dependencies": {"@xmtp/node-sdk": pin},
                    }
                    if extra:
                        manifest["dependencies"]["viem"] = "^2.55.10"
                    self.write_json(path + "/package.json", manifest)
                    inputs = [
                        self.root / path / "package.json",
                        self.root / "pnpm-lock.yaml",
                        agent,
                        node,
                    ]
                    before = [file.read_bytes() for file in inputs]
                    result = self.run_helper(sdk, agent, node)
                    self.assertEqual([file.read_bytes() for file in inputs], before)
                    if pin == VERSION and not extra:
                        self.assertEqual(
                            result.returncode, 0, result.stdout + result.stderr
                        )
                    else:
                        self.assertNotEqual(
                            result.returncode, 0, result.stdout + result.stderr
                        )
                        self.assertIn(
                            "ERR_PNPM_OUTDATED_LOCKFILE", result.stdout + result.stderr
                        )

    def start_registry(self):
        self.payloads = {}
        self.active = {"viem": ["2.56.5"], "locked-leaf": ["1.0.0"]}
        self.requests = []
        for name, versions in {
            "viem": ("2.56.5", "2.56.6"),
            "locked-leaf": ("1.0.0", "1.0.1"),
        }.items():
            for version in versions:
                manifest = {
                    "name": name,
                    "version": version,
                    "type": "module",
                    "main": "index.js",
                }
                source = f"export const marker = '{name}={version}';"
                if name == "viem":
                    manifest["dependencies"] = {"locked-leaf": "^1.0.0"}
                    source = f"import {{marker as leaf}} from 'locked-leaf'; export const marker = 'viem={version} ' + leaf;"
                self.payloads[name, version] = archive(manifest, {"index.js": source})
        fixture = self

        class Registry(BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass

            def parse_request(self):
                accepted = super().parse_request()
                if accepted:
                    fixture.requests.append((self.command, self.path))
                return accepted

            def do_GET(self):
                name = self.path.strip("/")
                if name in fixture.active:
                    entries = {}
                    for version in fixture.active[name]:
                        entries[version] = {
                            "name": name,
                            "version": version,
                            "dist": {
                                "tarball": f"{fixture.registry}{name}/-/{name}-{version}.tgz",
                                "integrity": integrity(fixture.payloads[name, version]),
                            },
                        }
                        if name == "viem":
                            entries[version]["dependencies"] = {"locked-leaf": "^1.0.0"}
                    data = json.dumps(
                        {
                            "name": name,
                            "versions": entries,
                            "dist-tags": {"latest": fixture.active[name][-1]},
                            "time": {
                                version: "2020-01-01T00:00:00.000Z"
                                for version in entries
                            },
                        }
                    ).encode()
                else:
                    key = next(
                        (
                            key
                            for key in fixture.payloads
                            if self.path == f"/{key[0]}/-/{key[0]}-{key[1]}.tgz"
                        ),
                        None,
                    )
                    if key is None:
                        self.send_error(404)
                        return
                    data = fixture.payloads[key]
                self.send_response(200)
                self.send_header(
                    "Content-Type",
                    "application/json"
                    if name in fixture.active
                    else "application/octet-stream",
                )
                self.send_header("Content-Length", str(len(data)))
                self.end_headers()
                self.wfile.write(data)

            def do_POST(self):
                self.send_error(405)

        server = ThreadingHTTPServer(("127.0.0.1", 0), Registry)
        self.registry = f"http://127.0.0.1:{server.server_port}/"
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()

        def stop():
            server.shutdown()
            server.server_close()
            thread.join()
            self.assertTrue(self.requests)
            self.assertTrue(
                all(method == "GET" for method, _ in self.requests), self.requests
            )

        self.addCleanup(stop)
        self.workspace["catalog"] = {"viem": "^2.55.10"}
        self.workspace["minimumReleaseAge"] = 10080
        self.lock["catalogs"] = {
            "default": {"viem": {"specifier": "^2.55.10", "version": "2.56.5"}}
        }
        self.lock["packages"] = {}
        self.lock["snapshots"] = {}
        for name, version in (("viem", "2.56.5"), ("locked-leaf", "1.0.0")):
            key = f"{name}@{version}"
            self.lock["packages"][key] = {
                "resolution": {"integrity": integrity(self.payloads[name, version])}
            }
            self.lock["snapshots"][key] = (
                {"dependencies": {"locked-leaf": "1.0.0"}} if name == "viem" else {}
            )
        for sdk, path in (("agent-sdk", "sdks/agent"), ("cli", "apps/cli")):
            self.manifests[sdk]["dependencies"]["viem"] = "catalog:"
            self.write_json(path + "/package.json", self.manifests[sdk])
            self.lock["importers"][path]["dependencies"]["viem"] = {
                "specifier": "catalog:",
                "version": "2.56.5",
            }
        self.write_lock()

    def test_registry_drift_cannot_change_direct_or_transitive_runtime(self):
        self.start_registry()
        for sdk in ("agent-sdk", "cli"):
            agent, node = self.pack_sdks(sdk, dependency=True)
            for newer in (False, True):
                self.active["viem"] = ["2.56.5", "2.56.6"] if newer else ["2.56.5"]
                self.active["locked-leaf"] = ["1.0.0", "1.0.1"] if newer else ["1.0.0"]
                with self.subTest(sdk=sdk, newer=newer):
                    result = self.run_helper(sdk, agent, node)
                    self.assertEqual(
                        result.returncode, 0, result.stdout + result.stderr
                    )
                    self.assertIn("viem=2.56.5 locked-leaf=1.0.0", result.stdout)
                    self.assertNotIn("viem=2.56.6", result.stdout)
                    self.assertNotIn("locked-leaf=1.0.1", result.stdout)
        self.assertFalse(
            any(
                "2.56.6.tgz" in path or "1.0.1.tgz" in path for _, path in self.requests
            )
        )

    def test_registry_cannot_replace_reviewed_direct_or_transitive_bytes(self):
        self.start_registry()
        agent, node = self.pack_sdks("agent-sdk", dependency=True)
        for key in (("viem", "2.56.5"), ("locked-leaf", "1.0.0")):
            original = self.payloads[key]
            manifest = {
                "name": key[0],
                "version": key[1],
                "type": "module",
                "main": "index.js",
            }
            if key[0] == "viem":
                manifest["dependencies"] = {"locked-leaf": "^1.0.0"}
            self.payloads[key] = archive(
                manifest, {"index.js": "export const marker = 'registry replacement';"}
            )
            with self.subTest(package=key[0]):
                result = self.run_helper("agent-sdk", agent, node)
                self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertIn(
                    "ERR_PNPM_TARBALL_INTEGRITY", result.stdout + result.stderr
                )
            self.payloads[key] = original


if __name__ == "__main__":
    unittest.main()
