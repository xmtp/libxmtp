"""Check the host wrapper with fake sbx and Orca processes, never a live VM."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[2]
NAME = "libxmtp-claude"
KEY = "test-key-kept-off-the-command-line"
FAKE = """#!/usr/bin/env python3.11
import json, os, pathlib, re, sys
path = pathlib.Path(os.environ["LIBXMTP_SBX_TEST_STATE"])
s = json.loads(path.read_text())
a = sys.argv[1:]
kind = pathlib.Path(sys.argv[0]).name
call = {"kind": kind, "args": a}
s["calls"].append(call)
if kind == "sbx":
    if a[:2] == ["ls", "--json"]:
        print(json.dumps(s["vms"]))
    elif a[:2] == ["mcp", "ls"]:
        print(json.dumps(s["mcp_servers"]))
    elif a[:1] == ["inspect"]:
        print(json.dumps(next(row for row in s["vms"] if row["name"] == a[1])))
    elif a[:2] == ["settings", "get"]:
        print("false")
    elif a[:2] == ["env", "run"]:
        own = next((row for row in s["vms"] if row["name"] == "libxmtp-claude"), None)
        if own is None:
            s["vms"].append({"name": "libxmtp-claude", "status": "running", **s["details"]})
            s["disk"] = {"files": "new", "cache": "new"}
            s["bootstrap_complete"] = True
        else:
            own["status"] = "running"
    elif a[:1] == ["exec"] and a[2:4] == ["bash", "-c"]:
        path.write_text(json.dumps(s))
        sys.exit(0 if s["bootstrap_complete"] else 1)
    elif a[:2] == ["env", "rm"]:
        if any(row["name"] == "libxmtp-claude" for row in s["vms"]):
            s["disk"], s["keys"] = None, []
        s["vms"] = [row for row in s["vms"] if row["name"] != "libxmtp-claude"]
    elif a[:1] == ["reset"]:
        s["vms"], s["disk"], s["keys"], s["history"] = [], None, [], None
    elif a[:1] == ["stop"]:
        for row in s["vms"]:
            if row["name"] == a[-1]:
                row["status"] = "running" if s["attached"] else "stopped"
    elif a[:2] == ["secret", "ls"]:
        secrets = [{"scope": "(global)", "service": "openai"}] if s["openai_secret"] else []
        print(json.dumps({"secrets": secrets, "custom_secrets": []}))
    elif a[:3] == ["secret", "set", "openai"]:
        if s.get("fail_oauth"):
            path.write_text(json.dumps(s))
            sys.exit(1)
        s["openai_secret"] = True
    elif a[:2] == ["secret", "set-custom"]:
        if not re.fullmatch("[A-Z_][A-Z0-9_]*", a[a.index("--env") + 1]):
            path.write_text(json.dumps(s))
            sys.exit(1)
        call["stdin"] = sys.stdin.read()
        s["keys"].append(call["stdin"].strip())
elif kind == "orca":
    if a[:1] == ["status"]:
        remote = "--environment" in a
        ready = not (remote and s.get("fail_ready"))
        print(json.dumps({"ok": True, "result": {"runtime": {"state": "ready" if ready else "starting", "reachable": ready}}}))
    if a[:2] == ["environment", "decommission"]:
        s["disk"] = None
elif kind == "setup":
    call["sandbox"] = os.environ.get("SBX_SANDBOX_NAME")
    s["bootstrap_complete"] = not s.get("fail_setup", False)
    path.write_text(json.dumps(s))
    sys.exit(0 if s["bootstrap_complete"] else 1)
else:
    action = a[1]
    if action == "ensure":
        s["target"], s["attached"] = True, True
    present = s["target"]
    if action == "remove":
        s["target"], s["attached"] = False, False
    print(json.dumps({"targetId": "own-target" if present else None, "environmentId": "own-env" if present else None}))
path.write_text(json.dumps(s))
"""


class SandboxTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="xmtp-sbx-test-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()
        self.tools = self.root / "dev/sbx"
        self.tools.mkdir(parents=True)
        for name in ("sandbox", "sbxenv.yaml"):
            shutil.copy2(ROOT / "dev/sbx" / name, self.tools / name)
        self.declaration = yaml.safe_load((self.tools / "sbxenv.yaml").read_text())
        self.state_file = self.root / "state.json"
        self.save(
            {
                "vms": [],
                "details": {
                    "agent": self.declaration["agent"],
                    "kits": self.declaration["kits"],
                    "runtime_mounts": [],
                },
                "mcp_servers": [],
                "openai_secret": False,
                "bootstrap_complete": False,
                "disk": None,
                "keys": [],
                "attached": False,
                "target": True,
                "calls": [],
            }
        )
        binary = self.root / "bin"
        binary.mkdir()
        app = self.root / "FakeOrca.app/Contents"
        for executable in (
            binary / "sbx",
            self.tools / "setup",
            app / "Resources/bin/orca",
            app / "MacOS/Orca",
        ):
            executable.parent.mkdir(parents=True, exist_ok=True)
            executable.write_text(FAKE)
            executable.chmod(0o755)
        (binary / "orca").symlink_to(app / "Resources/bin/orca")
        self.env = {
            **os.environ,
            "PATH": f"{binary}:{os.environ['PATH']}",
            "LIBXMTP_SBX_TEST_STATE": str(self.state_file),
        }

    def save(self, state):
        self.state_file.write_text(json.dumps(state))

    def state(self):
        return json.loads(self.state_file.read_text())

    def command(self, *args, key=""):
        return subprocess.run(
            ["bash", str(self.tools / "sandbox"), *args],
            input=key,
            capture_output=True,
            text=True,
            env=self.env,
        )

    def test_foreign_or_multiple_vms_are_not_mutated(self):
        for names in (("other",), (NAME, "other")):
            for action in ("start", "pause", "destroy"):
                with self.subTest(names=names, action=action):
                    state = self.state()
                    state.update(
                        vms=[{"name": n, "status": "running"} for n in names], calls=[]
                    )
                    self.save(state)
                    self.assertNotEqual(self.command(action).returncode, 0)
                    self.assertEqual(self.state()["vms"], state["vms"])
                    self.assertEqual(
                        {k: v for k, v in self.state().items() if k != "calls"},
                        {k: v for k, v in state.items() if k != "calls"},
                    )

    def test_environment_has_no_host_mounts_or_shared_skills(self):
        for field in ("workspace", "additionalWorkspaces", "mounts"):
            with self.subTest(field=field):
                self.assertNotIn(field, self.declaration)
        self.assertEqual(self.declaration["sandboxOptions"]["skills"], "off")

    def test_start_creates_once_and_resumes_the_same_disk(self):
        self.assertEqual(self.command("start").returncode, 0)
        state = self.state()
        run = next(
            call for call in state["calls"] if call["args"][:2] == ["env", "run"]
        )
        self.assertEqual(run["args"][-1], str(self.tools / "sbxenv.yaml"))
        self.assertTrue(Path(run["args"][-1]).is_absolute())
        self.assertEqual(len(state["vms"]), 1)
        state["disk"] = {"files": "agent work", "cache": "Nix cache"}
        self.save(state)
        self.assertEqual(self.command("start").returncode, 0)
        self.assertEqual(self.state()["disk"], state["disk"])
        self.assertEqual(len(self.state()["vms"]), 1)

    def sign_ins(self):
        return [
            index
            for index, call in enumerate(self.state()["calls"])
            if call["args"] == ["secret", "set", "openai", "--oauth"]
        ]

    def test_new_vm_gets_openai_sign_in_before_creation(self):
        result = self.command("start")
        self.assertEqual(result.returncode, 0, result.stderr)
        calls = self.state()["calls"]
        lookup = next(call for call in calls if call["args"][:2] == ["secret", "ls"])
        self.assertIn("--global", lookup["args"])
        self.assertEqual(
            lookup["args"][lookup["args"].index("--service") + 1], "openai"
        )
        creation = next(
            index
            for index, call in enumerate(calls)
            if call["args"][:2] == ["env", "run"]
        )
        self.assertEqual(len(self.sign_ins()), 1)
        self.assertLess(self.sign_ins()[0], creation)

    def test_existing_openai_secret_or_vm_skips_sign_in(self):
        for case in ("global secret", "existing VM"):
            with self.subTest(case=case):
                state = self.state()
                state.update(calls=[], vms=[], openai_secret=False)
                if case == "global secret":
                    state["openai_secret"] = True
                else:
                    state.update(
                        vms=[{"name": NAME, "status": "stopped", **state["details"]}],
                        bootstrap_complete=True,
                    )
                self.save(state)
                result = self.command("start")
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(self.sign_ins(), [])

    def test_failed_openai_sign_in_creates_nothing(self):
        state = self.state()
        state["fail_oauth"] = True
        self.save(state)
        result = self.command("start")
        self.assertNotEqual(result.returncode, 0)
        self.assertNotIn("Ready:", result.stdout)
        self.assertEqual(self.state()["vms"], [])
        self.assertFalse(
            any(call["args"][:2] == ["env", "run"] for call in self.state()["calls"])
        )

    def test_host_mcp_registration_blocks_start_before_creation(self):
        state = self.state()
        state["mcp_servers"] = [{"name": "host-files"}]
        self.save(state)
        result = self.command("start")
        self.assertNotEqual(result.returncode, 0)
        self.assertNotIn("Ready:", result.stdout)
        self.assertEqual(self.state()["vms"], [])
        self.assertFalse(
            any(call["args"][:2] == ["env", "run"] for call in self.state()["calls"])
        )

    def test_incompatible_same_name_vm_is_not_used(self):
        for mismatch in (
            {"runtime_mounts": [{"source": "/host/private", "target": "/workspace"}]},
            {"workspace": "/host/private", "mounts": []},
            {"mounts": [{"source": "/host/private", "target": "/workspace"}]},
            {"agent": "claude"},
            {"kits": ["docker.io/docker/sbx-kit-claude:older"]},
        ):
            for args in (
                ("start",),
                ("destroy",),
                ("key", "api.example.com", "sbx-test"),
            ):
                with self.subTest(mismatch=mismatch, command=args):
                    state = self.state()
                    state.update(
                        vms=[
                            {
                                "name": NAME,
                                "status": "stopped",
                                **state["details"],
                                **mismatch,
                            }
                        ],
                        disk="preserved work",
                        calls=[],
                    )
                    self.save(state)
                    result = self.command(*args, key=KEY + "\n")
                    self.assertNotEqual(result.returncode, 0)
                    self.assertNotIn("Ready:", result.stdout)
                    self.assertNotIn("API key for ", result.stderr)
                    self.assertFalse(
                        any(
                            call["args"][:2]
                            in (["policy", "allow"], ["secret", "set-custom"])
                            for call in self.state()["calls"]
                        )
                    )
                    self.assertEqual(
                        {k: v for k, v in self.state().items() if k != "calls"},
                        {k: v for k, v in state.items() if k != "calls"},
                    )

    def incomplete_bootstrap(self, *, fail_setup=False):
        state = self.state()
        state.update(
            vms=[{"name": NAME, "status": "stopped", **state["details"]}],
            disk="preserved work",
            bootstrap_complete=False,
            fail_setup=fail_setup,
        )
        self.save(state)

    def test_start_repairs_an_incomplete_existing_bootstrap(self):
        self.incomplete_bootstrap()
        result = self.command("start")
        self.assertEqual(result.returncode, 0, result.stderr)
        state = self.state()
        self.assertTrue(state["bootstrap_complete"])
        self.assertEqual(state["disk"], "preserved work")
        setup = [call for call in state["calls"] if call["kind"] == "setup"]
        self.assertEqual(len(setup), 1)
        self.assertEqual(setup[0]["sandbox"], NAME)
        probes = [call for call in state["calls"] if call["args"][:1] == ["exec"]]
        self.assertEqual(len(probes), 2)
        self.assertIn("Ready:", result.stdout)

    def test_failed_bootstrap_is_not_reported_as_ready(self):
        self.incomplete_bootstrap(fail_setup=True)
        result = self.command("start")
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(self.state()["bootstrap_complete"])
        self.assertNotIn("Ready:", result.stdout)
        self.assertFalse(self.state()["attached"])

    def test_pause_prevents_autowake_and_destroy_removes_disk_and_keys(self):
        self.assertEqual(self.command("start").returncode, 0)
        state = self.state()
        state.update(
            disk={"files": "agent work", "cache": "Nix cache"},
            keys=["stored-key"],
            history="native retained Claude history",
        )
        self.save(state)
        self.assertEqual(self.command("pause").returncode, 0)
        paused = self.state()
        self.assertEqual(paused["vms"][0]["status"], "stopped")
        self.assertEqual(paused["disk"], state["disk"])
        self.assertEqual(paused["keys"], state["keys"])
        self.assertEqual(self.command("destroy").returncode, 0)
        deleted = self.state()
        self.assertEqual(deleted["vms"], [])
        self.assertIsNone(deleted["disk"])
        self.assertEqual(deleted["keys"], [])
        self.assertEqual(deleted["history"], state["history"])

    def test_pause_without_an_orca_target_does_not_create_one(self):
        state = self.state()
        state.update(
            vms=[{"name": NAME, "status": "running"}], target=False, disk="work"
        )
        self.save(state)
        self.assertEqual(self.command("pause").returncode, 0)
        self.assertFalse(self.state()["target"])
        self.assertEqual(self.state()["disk"], "work")

    def test_real_key_goes_only_to_host_proxy_stdin(self):
        self.assertEqual(self.command("start").returncode, 0)
        result = self.command("key", "api.example.com", "sbx-test", key=KEY + "\n")
        self.assertEqual(result.returncode, 0, result.stderr)
        state = self.state()
        self.assertEqual(state["keys"], [KEY])
        self.assertNotIn(KEY, result.stdout + result.stderr)
        for call in state["calls"]:
            self.assertNotIn(KEY, " ".join(call["args"]))
            if "stdin" in call:
                self.assertEqual(call["args"][:2], ["secret", "set-custom"])
                self.assertEqual(call["stdin"], KEY + "\n")
                for flag, value in (
                    ("--sandbox", NAME),
                    ("--host", "api.example.com"),
                    ("--placeholder", "sbx-test"),
                ):
                    self.assertIn(flag, call["args"])
                    self.assertEqual(call["args"][call["args"].index(flag) + 1], value)

    def test_start_registers_repeated_keys_in_order_without_exposing_them(self):
        second_key = "second-key-kept-off-the-command-line"
        result = self.command(
            "start",
            "--api-key",
            "api.first.example",
            "sbx-first",
            "--api-key",
            "api.second.example",
            "sbx-second",
            key=f"{KEY}\n{second_key}\n",
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        state = self.state()
        self.assertEqual(state["keys"], [KEY, second_key])
        registrations = [
            call
            for call in state["calls"]
            if call["kind"] == "sbx" and call["args"][:2] == ["secret", "set-custom"]
        ]
        self.assertEqual(len(registrations), 2)
        for call, host, placeholder, key in zip(
            registrations,
            ("api.first.example", "api.second.example"),
            ("sbx-first", "sbx-second"),
            (KEY, second_key),
        ):
            self.assertEqual(call["stdin"], key + "\n")
            for flag, value in (
                ("--sandbox", NAME),
                ("--host", host),
                ("--placeholder", placeholder),
            ):
                self.assertEqual(call["args"][call["args"].index(flag) + 1], value)
        prompts = [
            f"API key for {host} [{placeholder}] (stored on host): "
            for host, placeholder in (
                ("api.first.example", "sbx-first"),
                ("api.second.example", "sbx-second"),
            )
        ]
        self.assertIn(prompts[0], result.stderr)
        self.assertIn(prompts[1], result.stderr)
        self.assertLess(
            result.stderr.index(prompts[0]), result.stderr.index(prompts[1])
        )
        for key in (KEY, second_key):
            self.assertNotIn(key, result.stdout + result.stderr)
            for call in state["calls"]:
                self.assertNotIn(key, " ".join(call["args"]))

    def test_malformed_later_key_option_is_rejected_before_mutation(self):
        for tail in (
            ("--api-key", "api.second.example"),
            ("--api-key",),
            ("--unknown", "api.second.example", "sbx-second"),
        ):
            with self.subTest(tail=tail):
                state = self.state()
                state["calls"] = []
                self.save(state)
                result = self.command(
                    "start", "--api-key", "api.first.example", "sbx-first", *tail
                )
                self.assertEqual(result.returncode, 2)
                self.assertEqual(self.state()["vms"], [])
                self.assertEqual(self.state()["keys"], [])
                self.assertFalse(
                    any(
                        call["args"][:2] in (["env", "run"], ["secret", "set-custom"])
                        for call in self.state()["calls"]
                    )
                )

    def test_empty_key_is_not_registered(self):
        self.assertEqual(self.command("start").returncode, 0)
        result = self.command("key", "api.example.com", "sbx-test", key="\n")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.state()["keys"], [])

    def test_orca_not_ready_is_not_reported_as_success(self):
        state = self.state()
        state["fail_ready"] = True
        self.save(state)
        result = self.command("start")
        self.assertNotEqual(result.returncode, 0)
        self.assertNotIn("Ready:", result.stdout)


if __name__ == "__main__":
    unittest.main()
