"""Check fixture ownership and cleanup with local process stubs."""

import os
from pathlib import Path
import signal
import subprocess
import tempfile
import time
import unittest


FIXTURE = Path(__file__).with_name("metadata_backend.py")


class MetadataBackendFixtureTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.env = dict(os.environ, DATABASE_URL="postgres://test:test@127.0.0.1:1/shared",
                        XMTP_S3_URL="http://127.0.0.1:2", XMTP_S3_BASE_URL="http://127.0.0.1:2/attachments",
                        XMTP_METADATA_LOG_DIR=str(self.root / "logs"), FIXTURE_LEDGER=str(self.root / "ledger"))
        self.env["PATH"] = str(self.root) + os.pathsep + self.env["PATH"]
        self.executable("psql", """import os,sys
with open(os.environ['FIXTURE_LEDGER'], 'a') as out: out.write(sys.argv[-1] + '\\n')
if os.environ.get('FAIL_CREATE') and sys.argv[-1].startswith('CREATE'): sys.exit(9)
""")
        self.executable("grpc-health-probe", """import os,sys
sys.exit(1 if os.environ.get('FAIL_BACKEND') else 0)
""")
        self.backend = self.executable("backend", """import os,signal,sys,time,tomllib
from pathlib import Path
cfg=tomllib.loads(Path(sys.argv[-1]).read_text())
assert len(cfg['application_components']) == 8
assert cfg['server']['listen'].startswith('127.0.0.1:')
if os.environ.get('FAIL_BACKEND'): sys.exit(8)
def end(*_):
    with open(os.environ['FIXTURE_LEDGER'], 'a') as out: out.write('BACKEND_STOP\\n')
    sys.exit(0)
signal.signal(signal.SIGTERM,end)
with open(os.environ['FIXTURE_LEDGER'], 'a') as out: out.write('BACKEND_READY\\n')
while True: time.sleep(.05)
""")
        self.child = self.executable("child", """import os,sys,time
from pathlib import Path
assert os.environ['XMTP_ANDROID_BACKEND_URL'] == os.environ['XMTP_BACKEND_URL']
assert os.environ['XMTP_REPLICA_URL'] == os.environ['DATABASE_URL']
with open(os.environ['FIXTURE_LEDGER'], 'a') as out: out.write('CHILD_READY\\n')
if os.environ.get('WAIT_CHILD'):
    while True: time.sleep(.05)
time.sleep(.1)
sys.exit(int(os.environ.get('CHILD_STATUS','0')))
""")

    def tearDown(self):
        self.temp.cleanup()

    def executable(self, name, source):
        import sys
        path = self.root / name
        path.write_text(f"#!{sys.executable}\n" + source)
        path.chmod(0o700)
        return str(path)

    def start(self, **changes):
        import sys
        return subprocess.Popen([sys.executable, str(FIXTURE), "--backend", self.backend, "--", self.child], env=dict(self.env, **changes), stdout=subprocess.PIPE, stderr=subprocess.PIPE)

    def check_cleanup(self):
        ledger = (self.root / "ledger").read_text().splitlines()
        create = next(line for line in ledger if line.startswith("CREATE"))
        drop = next(line for line in ledger if line.startswith("DROP"))
        self.assertEqual(create.split('"')[1], drop.split('"')[1])
        self.assertTrue(create.split('"')[1].startswith("messenger_metadata_"))
        self.assertNotIn('"shared"', drop)
        return ledger

    def test_failed_command_preserves_status_and_cleans_owned_resources(self):
        process = self.start(CHILD_STATUS="7")
        out, error = process.communicate(timeout=15)
        self.assertEqual(7, process.returncode, (out, error))
        self.assertIn("BACKEND_STOP", self.check_cleanup())

    def test_backend_startup_failure_drops_only_owned_database(self):
        process = self.start(FAIL_BACKEND="1")
        process.communicate(timeout=15)
        self.assertNotEqual(0, process.returncode)
        self.assertNotIn("CHILD_READY", self.check_cleanup())

    def test_signal_stops_child_backend_and_drops_database(self):
        process = self.start(WAIT_CHILD="1")
        try:
            deadline = time.monotonic() + 10
            while not (self.root / "ledger").exists() or "CHILD_READY" not in (self.root / "ledger").read_text():
                if time.monotonic() >= deadline:
                    self.fail("Child did not start")
                time.sleep(.02)
            process.send_signal(signal.SIGTERM)
            process.communicate(timeout=15)
            self.assertEqual(143, process.returncode)
            self.assertIn("BACKEND_STOP", self.check_cleanup())
        finally:
            if process.poll() is None:
                process.kill()
                process.communicate(timeout=15)

    def test_failed_database_creation_does_not_drop_a_database(self):
        process = self.start(FAIL_CREATE="1")
        process.communicate(timeout=15)
        self.assertNotEqual(0, process.returncode)
        self.assertNotIn("DROP", (self.root / "ledger").read_text())


if __name__ == "__main__":
    unittest.main()
