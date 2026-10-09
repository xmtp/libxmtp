"""Check fixture ownership and cleanup with local process stubs."""

import os
from pathlib import Path
import signal
import subprocess
import tempfile
import time
import unittest
from unittest.mock import Mock, patch
from urllib.parse import quote, urlsplit

from metadata_backend import psql_connection, stop
from process_group import KeeperError, OwnedProcess


FIXTURE = Path(__file__).with_name("metadata_backend.py")


class MetadataBackendFixtureTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.password = "fixture:password\\with@marks%"
        self.env = dict(
            os.environ,
            DATABASE_URL=f"postgres://test:{quote(self.password, safe='')}@127.0.0.1:1/shared",
            XMTP_DATABASE_URL="postgres://test:test@127.0.0.1:1/shared_alias",
            XMTP_REPLICA_URL="postgres://test:test@127.0.0.1:1/shared_replica",
            XMTP_S3_URL="http://127.0.0.1:2",
            XMTP_S3_BASE_URL="http://127.0.0.1:2/attachments",
            XMTP_BACKEND_URL="http://127.0.0.1:3",
            XMTP_ANDROID_BACKEND_URL="http://10.0.2.2:3",
            XMTP_METADATA_LOG_DIR=str(self.root / "logs"),
            FIXTURE_LEDGER=str(self.root / "ledger"),
            FIXTURE_PASSWORD=self.password,
            PGPASSWORD="caller-password-must-not-override-uri",
        )
        self.env["PATH"] = str(self.root) + os.pathsep + self.env["PATH"]
        self.executable(
            "psql",
            """import os,sys,time,stat,subprocess
from pathlib import Path
from urllib.parse import parse_qsl,urlsplit
connection=next(value for value in sys.argv if value.startswith(('postgres://','postgresql://')))
assert urlsplit(connection).password is None, 'Password in psql argv'
assert not any(key == 'password' for key,value in parse_qsl(urlsplit(connection).query)), 'Password parameter in psql argv'
assert '--no-password' in sys.argv
passfile=Path(os.environ['PGPASSFILE'])
assert stat.S_IMODE(passfile.stat().st_mode) == 0o600
assert stat.S_IMODE(passfile.parent.stat().st_mode) == 0o700
password=os.environ['FIXTURE_PASSWORD'].replace('\\\\','\\\\\\\\').replace(':','\\\\:')
assert passfile.read_text() == '*:*:*:*:' + password + '\\n'
assert 'PGPASSWORD' not in os.environ
assert not any(name in os.environ for name in ('DATABASE_URL','XMTP_DATABASE_URL','XMTP_REPLICA_URL'))
statement=sys.argv[-1]
state=Path(os.environ['FIXTURE_LEDGER'] + '.database')
with open(os.environ['FIXTURE_LEDGER'], 'a') as out:
    out.write('PSQL_PASSFILE ' + str(passfile) + '\\n')
    out.write(statement + '\\n')
if statement.startswith('SELECT'):
    print('t' if state.exists() else 'f')
elif statement.startswith('CREATE'):
    if os.environ.get('FAIL_CREATE'): sys.exit(9)
    state.write_text(statement.split('"')[1])
    if os.environ.get('WAIT_CREATE'):
        with open(os.environ['FIXTURE_LEDGER'], 'a') as out: out.write('CREATE_COMMITTED\\n')
        time.sleep(1)
    if os.environ.get('UNCERTAIN_CREATE'): sys.exit(8)
elif statement.startswith('DROP'):
    assert statement.split('"')[1] == state.read_text()
    descendant=Path(os.environ['FIXTURE_LEDGER'] + '.descendant')
    if descendant.exists():
        pid,group=map(int,descendant.read_text().split())
        status=subprocess.run(['ps','-p',str(pid),'-o','stat='],capture_output=True,text=True).stdout.strip()
        if status and not status.startswith('Z'):
            with open(os.environ['FIXTURE_LEDGER'],'a') as out: out.write('DESCENDANT_RUNNING_AT_DROP\\n')
    state.unlink()
""",
        )
        self.executable(
            "grpc-health-probe",
            """import os,sys
sys.exit(1 if os.environ.get('FAIL_BACKEND') else 0)
""",
        )
        self.backend = self.executable(
            "backend",
            """import os,signal,sys,time,tomllib
from pathlib import Path
from urllib.parse import parse_qsl,unquote,urlsplit
def effective_database(value):
    uri=urlsplit(value)
    result=unquote(uri.path.removeprefix('/'))
    for key,value in parse_qsl(uri.query,keep_blank_values=True):
        if key == 'dbname': result=value
    return result
cfg=tomllib.loads(Path(sys.argv[-1]).read_text())
created=Path(os.environ['FIXTURE_LEDGER']+'.database').read_text()
assert all(effective_database(os.environ[key]) == created for key in ('DATABASE_URL','XMTP_DATABASE_URL','XMTP_REPLICA_URL'))
assert len(cfg['application_components']) == 12
assert cfg['server']['listen'].startswith('127.0.0.1:')
if os.environ.get('FAIL_BACKEND'): sys.exit(8)
def end(*_):
    with open(os.environ['FIXTURE_LEDGER'], 'a') as out: out.write('BACKEND_STOP\\n')
    sys.exit(0)
signal.signal(signal.SIGTERM,end)
with open(os.environ['FIXTURE_LEDGER'], 'a') as out: out.write('BACKEND_READY\\n')
while True: time.sleep(.05)
""",
        )
        self.child = self.executable(
            "child",
            """import os,sys,time,subprocess,json,stat
from pathlib import Path
from urllib.parse import parse_qsl,unquote,urlsplit
assert os.environ['XMTP_ANDROID_BACKEND_URL'] == 'http://10.0.2.2:3'
assert os.environ['XMTP_BACKEND_URL'] == 'http://127.0.0.1:3'
assert os.environ['XMTP_S3_URL'] == 'http://127.0.0.1:2'
assert os.environ['XMTP_S3_BASE_URL'] == 'http://127.0.0.1:2/attachments'
owned_name = Path(os.environ['FIXTURE_LEDGER'] + '.database').read_text()
assert urlsplit(os.environ['DATABASE_URL']).path == '/' + owned_name
for key in ('DATABASE_URL','XMTP_DATABASE_URL','XMTP_REPLICA_URL'):
    uri=urlsplit(os.environ[key])
    effective=unquote(uri.path.removeprefix('/'))
    for name,value in parse_qsl(uri.query,keep_blank_values=True):
        if name == 'dbname': effective=value
    assert effective == owned_name, 'SQL query overrides owned database'
assert os.environ['XMTP_DATABASE_URL'] == os.environ['DATABASE_URL']
assert os.environ['XMTP_REPLICA_URL'] == os.environ['DATABASE_URL']
assert os.environ['XMTP_METADATA_BACKEND_URL'] == 'http://127.0.0.1:' + os.environ['XMTP_METADATA_BACKEND_PORT']
lease_path=Path(os.environ['XMTP_METADATA_BACKEND_LEASE'])
lease=json.loads(lease_path.read_text())
assert stat.S_IMODE(lease_path.stat().st_mode) == 0o600
assert stat.S_IMODE(lease_path.parent.stat().st_mode) == 0o700
assert lease['formatVersion'] == 1 and lease['uid'] == os.getuid()
assert lease['url'] == os.environ['XMTP_METADATA_BACKEND_URL']
assert lease['port'] == int(os.environ['XMTP_METADATA_BACKEND_PORT'])
assert lease['database'] == owned_name
assert os.getpgid(lease['serverPid']) == lease['groupPid']
assert os.getsid(lease['serverPid']) == lease['groupPid']
os.kill(lease['ownerPid'],0)
with open(os.environ['FIXTURE_LEDGER'],'a') as out: out.write('LEASE ' + str(lease_path) + '\\n')
with open(os.environ['FIXTURE_LEDGER'], 'a') as out: out.write('CHILD_READY\\n')
if os.environ.get('SPAWN_DESCENDANT'):
    subprocess.Popen([sys.executable,'-c',"import os,signal,time; from pathlib import Path; signal.signal(signal.SIGTERM,signal.SIG_IGN); Path(os.environ['FIXTURE_LEDGER']+'.descendant').write_text(str(os.getpid())+' '+str(os.getpgrp())); time.sleep(60)"],stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    deadline=time.monotonic()+5
    while not Path(os.environ['FIXTURE_LEDGER']+'.descendant').exists():
        assert time.monotonic()<deadline, 'Descendant did not become ready'
        time.sleep(.01)
if os.environ.get('WAIT_CHILD'):
    while True: time.sleep(.05)
time.sleep(.1)
sys.exit(int(os.environ.get('CHILD_STATUS','0')))
""",
        )

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

        return subprocess.Popen(
            [sys.executable, str(FIXTURE), "--backend", self.backend, "--", self.child],
            env=dict(self.env, **changes),
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )

    def check_cleanup(self):
        ledger = (self.root / "ledger").read_text().splitlines()
        create = next(line for line in ledger if line.startswith("CREATE"))
        drops = [line for line in ledger if line.startswith("DROP")]
        self.assertTrue(drops, "Owned database drop is missing")
        drop = drops[0]
        self.assertEqual(create.split('"')[1], drop.split('"')[1])
        self.assertTrue(create.split('"')[1].startswith("messenger_metadata_"))
        self.assertNotIn('"shared"', drop)
        for line in ledger:
            if line.startswith("PSQL_PASSFILE "):
                passfile = Path(line.removeprefix("PSQL_PASSFILE "))
                self.assertFalse(passfile.exists())
                self.assertFalse(passfile.parent.exists())
            elif line.startswith("LEASE "):
                self.assertFalse(Path(line.removeprefix("LEASE ")).exists())
        return ledger

    def test_failed_command_preserves_status_and_cleans_owned_resources(self):
        process = self.start(CHILD_STATUS="7")
        out, error = process.communicate(timeout=15)
        self.assertEqual(7, process.returncode, (out, error))
        self.assertIn("BACKEND_STOP", self.check_cleanup())

    def test_child_uses_owned_sql_urls_and_preserves_app_endpoints(self):
        process = self.start()
        out, error = process.communicate(timeout=15)
        self.assertEqual(0, process.returncode, (out, error))
        self.assertIn("CHILD_READY", self.check_cleanup())

    def test_decoded_database_query_cannot_redirect_owned_backend_or_child(self):
        for query in (
            "dbname=shared&sslmode=disable&d%62name=other&application_name=fixture%2Bscope",
            "%64%62%6e%61%6d%65=peer&dbname=&sslmode=disable",
        ):
            with self.subTest(query=query):
                (self.root / "ledger").unlink(missing_ok=True)
                process = self.start(
                    DATABASE_URL=self.env["DATABASE_URL"] + "?" + query
                )
                out, error = process.communicate(timeout=15)
                self.assertEqual(0, process.returncode, (out, error))
                self.assertIn("CHILD_READY", self.check_cleanup())

    def test_every_psql_call_uses_a_private_passfile_without_argv_password(self):
        process = self.start()
        out, error = process.communicate(timeout=15)
        self.assertEqual(0, process.returncode, (out, error))
        ledger = self.check_cleanup()
        passfiles = [
            Path(line.removeprefix("PSQL_PASSFILE "))
            for line in ledger
            if line.startswith("PSQL_PASSFILE ")
        ]
        self.assertEqual(4, len(passfiles))
        self.assertEqual(1, len(set(passfiles)))
        self.assertFalse(passfiles[0].exists())
        self.assertFalse(passfiles[0].parent.exists())

    def test_exited_leader_does_not_leave_descendant_running_at_database_drop(self):
        process = self.start(SPAWN_DESCENDANT="1")
        descendant = self.root / "ledger.descendant"
        try:
            out, error = process.communicate(timeout=15)
            self.assertEqual(0, process.returncode, (out, error))
            pid, group = map(int, descendant.read_text().split())
            self.assertNotEqual(pid, group)
            ledger = self.check_cleanup()
            self.assertNotIn("DESCENDANT_RUNNING_AT_DROP", ledger)
            state = subprocess.run(
                ["ps", "-p", str(pid), "-o", "stat="],
                capture_output=True,
                text=True,
            ).stdout.strip()
            self.assertTrue(not state or state.startswith("Z"), state)
        finally:
            if process.poll() is None:
                process.kill()
                process.communicate(timeout=15)
            if descendant.exists():
                pid, group = map(int, descendant.read_text().split())
                try:
                    if os.getpgid(pid) == group:
                        os.kill(pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass

    def test_stop_rejects_a_process_that_does_not_own_its_group(self):
        process = Mock(pid=123)
        with (
            patch("metadata_backend.os.getpgid", return_value=456),
            patch("metadata_backend.os.killpg") as kill,
        ):
            with self.assertRaisesRegex(ValueError, "does not own"):
                stop(process)
            kill.assert_not_called()

    def test_keeper_status_keeps_identity_unreaped_and_stale_id_cannot_signal(self):
        import sys

        group = OwnedProcess([sys.executable, "-c", "raise SystemExit(7)"])
        try:
            with (
                patch.object(
                    group._keeper, "poll", side_effect=AssertionError("early poll")
                ),
                patch.object(
                    group._keeper, "wait", side_effect=AssertionError("early reap")
                ),
            ):
                self.assertEqual(7, group.wait(timeout=5))
                self.assertEqual(7, group.poll())
            self.assertIsNone(group._keeper.returncode)
            self.assertEqual(group.pid, os.getpgid(group.pid))
        finally:
            group.stop()
        self.assertIsNotNone(group._keeper.returncode)
        self.assertIsNone(group._control)
        self.assertIsNone(group._status)
        group._keeper.pid = 123456
        with patch("process_group.os.killpg") as kill:
            group.stop()
            with self.assertRaisesRegex(KeeperError, "lease has ended"):
                group._signal(signal.SIGKILL)
            kill.assert_not_called()

    def test_keeper_failure_keeps_identity_until_remaining_command_is_stopped(self):
        import sys

        marker = self.root / "resistant"
        release = self.root / "release-resistant"
        source = (
            "import signal,time\nfrom pathlib import Path\nsignal.signal(signal.SIGTERM,signal.SIG_IGN)\nPath("
            + repr(str(marker))
            + ").write_text('ready')\nwhile not Path("
            + repr(str(release))
            + ").exists(): time.sleep(.01)"
        )
        group = OwnedProcess([sys.executable, "-c", source])
        command_pid = group.command_pid
        try:
            deadline = time.monotonic() + 5
            while not marker.exists():
                self.assertLess(time.monotonic(), deadline)
                time.sleep(0.01)
            os.kill(group.pid, signal.SIGKILL)
            deadline = time.monotonic() + 5
            while True:
                try:
                    group.poll()
                except KeeperError:
                    break
                self.assertLess(time.monotonic(), deadline)
                time.sleep(0.01)
            self.assertIsNone(group._keeper.returncode)
        finally:
            group.stop()
            release.write_text("release")
        state = subprocess.run(
            ["ps", "-p", str(command_pid), "-o", "stat="],
            capture_output=True,
            text=True,
        ).stdout.strip()
        self.assertTrue(not state or state.startswith("Z"), state)
        self.assertIsNone(group._control)
        self.assertIsNone(group._status)

    def test_keeper_startup_and_partial_pipe_failure_close_owned_resources(self):
        spawned = []
        original = subprocess.Popen

        def spawn(*args, **options):
            process = original(*args, **options)
            spawned.append(process)
            return process

        with patch("process_group.subprocess.Popen", side_effect=spawn):
            with self.assertRaisesRegex(KeeperError, "could not start"):
                OwnedProcess([str(self.root / "missing-command")])
        self.assertEqual(1, len(spawned))
        self.assertIsNotNone(spawned[0].returncode)
        read_fd, write_fd = os.pipe()
        with patch(
            "process_group.os.pipe",
            side_effect=[(read_fd, write_fd), OSError("pipe failure")],
        ):
            with self.assertRaisesRegex(OSError, "pipe failure"):
                OwnedProcess(["unused"])
        for descriptor in (read_fd, write_fd):
            with self.assertRaises(OSError):
                os.fstat(descriptor)

    def test_keeper_owner_exit_closes_lease_and_stops_its_group(self):
        import sys

        group = OwnedProcess([sys.executable, "-c", "import time; time.sleep(60)"])
        command_pid = group.command_pid
        try:
            os.close(group._control)
            group._control = None
            deadline = time.monotonic() + 5
            while True:
                try:
                    group.poll()
                except KeeperError:
                    break
                self.assertLess(
                    time.monotonic(),
                    deadline,
                    "Owner channel exit left its leased group running",
                )
                time.sleep(0.01)
            self.assertIsNone(group._keeper.returncode)
        finally:
            group.stop()
        state = subprocess.run(
            ["ps", "-p", str(command_pid), "-o", "stat="],
            capture_output=True,
            text=True,
        ).stdout.strip()
        self.assertTrue(not state or state.startswith("Z"), state)

    def test_psql_password_options_escape_and_cleanup(self):
        with self.assertRaisesRegex(ValueError, "PostgreSQL URI"):
            with psql_connection("password=not-a-uri", self.env):
                self.fail("Non-URI credentials reached psql arguments")
        cases = (
            (
                f"postgres://user:{quote(self.password, safe='')}@127.0.0.1:1/shared?sslmode=require&application_name=fixture%2Bproof",
                self.password,
                "sslmode=require&application_name=fixture%2Bproof",
            ),
            (
                "postgres://user:old@127.0.0.1:1/shared?password=new%3A%5Cvalue&sslmode=disable&passfile=/caller-file",
                "new:\\value",
                "sslmode=disable",
            ),
        )
        for database, password, query in cases:
            with self.subTest(query=query):
                with psql_connection(database, self.env) as (command, env):
                    self.assertIsNone(urlsplit(command[-1]).password)
                    self.assertEqual(query, urlsplit(command[-1]).query)
                    self.assertEqual("/shared", urlsplit(command[-1]).path)
                    passfile = Path(env["PGPASSFILE"])
                    escaped = password.replace("\\", "\\\\").replace(":", "\\:")
                    self.assertEqual("*:*:*:*:" + escaped + "\n", passfile.read_text())
                    self.assertEqual(0o600, passfile.stat().st_mode & 0o777)
                    self.assertEqual(0o700, passfile.parent.stat().st_mode & 0o777)
                    self.assertNotIn("PGPASSWORD", env)
                self.assertFalse(passfile.exists())
                self.assertFalse(passfile.parent.exists())

    def test_backend_startup_failure_drops_only_owned_database(self):
        process = self.start(FAIL_BACKEND="1")
        process.communicate(timeout=15)
        self.assertNotEqual(0, process.returncode)
        self.assertNotIn("CHILD_READY", self.check_cleanup())

    def test_signal_stops_child_backend_and_drops_database(self):
        process = self.start(WAIT_CHILD="1")
        try:
            deadline = time.monotonic() + 10
            while (
                not (self.root / "ledger").exists()
                or "CHILD_READY" not in (self.root / "ledger").read_text()
            ):
                if time.monotonic() >= deadline:
                    self.fail("Child did not start")
                time.sleep(0.02)
            process.send_signal(signal.SIGTERM)
            process.communicate(timeout=15)
            self.assertEqual(143, process.returncode)
            self.assertIn("BACKEND_STOP", self.check_cleanup())
        finally:
            if process.poll() is None:
                process.kill()
                process.communicate(timeout=15)

    def test_signal_during_database_creation_cleans_committed_database(self):
        for signum in (signal.SIGINT, signal.SIGTERM):
            with self.subTest(signum=signum):
                (self.root / "ledger").unlink(missing_ok=True)
                process = self.start(WAIT_CREATE="1")
                try:
                    deadline = time.monotonic() + 10
                    while (
                        not (self.root / "ledger").exists()
                        or "CREATE_COMMITTED" not in (self.root / "ledger").read_text()
                    ):
                        if time.monotonic() >= deadline:
                            self.fail("CREATE did not commit")
                        time.sleep(0.02)
                    process.send_signal(signum)
                    process.communicate(timeout=15)
                    self.assertEqual(128 + signum, process.returncode)
                    self.check_cleanup()
                    self.assertFalse((self.root / "ledger.database").exists())
                    self.assertNotIn(
                        "BACKEND_READY", (self.root / "ledger").read_text()
                    )
                finally:
                    if process.poll() is None:
                        process.kill()
                        process.communicate(timeout=15)

    def test_uncertain_create_failure_cleans_exact_committed_database(self):
        process = self.start(UNCERTAIN_CREATE="1")
        process.communicate(timeout=15)
        self.assertNotEqual(0, process.returncode)
        self.check_cleanup()
        self.assertFalse((self.root / "ledger.database").exists())

    def test_failed_database_creation_does_not_drop_a_database(self):
        process = self.start(FAIL_CREATE="1")
        process.communicate(timeout=15)
        self.assertNotEqual(0, process.returncode)
        self.assertNotIn("DROP", (self.root / "ledger").read_text())


if __name__ == "__main__":
    unittest.main()
