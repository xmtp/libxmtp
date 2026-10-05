#!/usr/bin/env python3
"""Check the emulator clock helper with a delayed Android settings service."""

import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
HELPER = Path(
    os.environ.get("ANDROID_CLOCK_HELPER", ROOT / "nix/lib/android-sync-clock.sh")
)
ADB = r"""#!/usr/bin/env python3.11
import json
import os
from pathlib import Path
import sys

path = Path(os.environ["CLOCK_STATE"])
state = json.loads(path.read_text())
args = sys.argv[3:]
if args in (["root"], ["wait-for-device"]):
    pass
elif args == ["shell", "service", "check", "settings"]:
    state["checks"] += 1
    state["ready"] = state["mode"] not in ("missing", "api23") and state["checks"] > state["delay"]
    print("Service settings: " + ("found" if state["ready"] else "not found"))
elif args == ["shell", "settings", "get", "global", "auto_time"]:
    state["checks"] += 1
    state["ready"] = state["mode"] != "missing" and state["checks"] > state["delay"]
    if state["ready"]:
        print("1\r")
    else:
        print("Error while accessing settings provider", file=sys.stderr)
elif args == ["shell", "settings", "put", "global", "auto_time", "0"]:
    if not state["ready"]:
        print("cmd: Can't find service: settings", file=sys.stderr)
        sys.exit(20)
    state["writes"] += 1
elif args[:3] == ["shell", "date", "-u"] and args[3].startswith("@"):
    state["sets"] += 1
    state["time"] = int(args[3][1:])
elif args == ["shell", "date", "-u", "+%s"]:
    state["reads"] += 1
    print(state["time"] - (10 if state["mode"] == "wrong-clock" else 0))
else:
    raise SystemExit("Unexpected adb command: " + repr(args))
path.write_text(json.dumps(state))
"""


class AndroidClockTest(unittest.TestCase):
    def run_clock(self, mode, delay=0):
        with tempfile.TemporaryDirectory() as name:
            directory = Path(name)
            adb = directory / "adb"
            adb.write_text(ADB)
            adb.chmod(0o755)
            sleep = directory / "sleep"
            sleep.write_text("#!/usr/bin/env bash\nexit 0\n")
            sleep.chmod(0o755)
            state = directory / "state.json"
            state.write_text(
                json.dumps(
                    dict(
                        mode=mode,
                        delay=delay,
                        ready=mode != "missing" and delay == 0,
                        checks=0,
                        writes=0,
                        sets=0,
                        reads=0,
                        time=0,
                    )
                )
            )
            env = dict(
                os.environ,
                CLOCK_STATE=str(state),
                PATH=str(directory) + os.pathsep + os.environ["PATH"],
            )
            result = subprocess.run(
                ["bash", str(HELPER), str(adb), "emulator-fixture"],
                env=env,
                text=True,
                capture_output=True,
                timeout=15,
            )
            return result, json.loads(state.read_text())

    def test_api23_content_provider_without_settings_binder_service(self):
        result, state = self.run_clock("api23")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual((state["writes"], state["sets"], state["reads"]), (1, 1, 1))

    def test_delayed_settings_service(self):
        result, state = self.run_clock("ready", delay=2)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(state["checks"], 3)
        self.assertEqual((state["writes"], state["sets"], state["reads"]), (1, 1, 1))

    def test_missing_settings_service_stops_before_clock(self):
        result, state = self.run_clock("missing")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("settings service did not become ready", result.stderr)
        self.assertEqual(state["checks"], 30)
        self.assertEqual((state["writes"], state["sets"], state["reads"]), (0, 0, 0))

    def test_wrong_clock_still_fails(self):
        result, state = self.run_clock("wrong-clock")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(
            "clock differs from the host by more than 2 seconds", result.stderr
        )
        self.assertEqual((state["writes"], state["sets"], state["reads"]), (1, 3, 3))


if __name__ == "__main__":
    unittest.main()
