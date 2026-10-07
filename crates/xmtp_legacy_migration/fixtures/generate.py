"""Build controlled history fixtures around actual legacy MLS storage.

Run in Nix. metadata-seed.db3 must come from producer.rs compiled at the
source revision recorded in README.md. This script never uses current SDK code.
"""

import ctypes as C
import hashlib
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import sys

HERE = Path(__file__).resolve().parent
KEY = "11" * 32
SALT = "22" * 16
EXPECTED = json.loads((HERE / "metadata-expected.json").read_text())
GROUP = EXPECTED["group_id"]
OWNER = EXPECTED["creator_inbox_id"]
PEER = EXPECTED["dm_members"]["member_two_inbox_id"]
CONTENT = subprocess.run(
    [
        "protoc",
        "--proto_path=" + str(HERE.parents[2] / "proto"),
        "--encode=xmtp.message_contents.EncodedContent",
        "message_contents/content.proto",
    ],
    input=b'type { authority_id: "xmtp.org" type_id: "text" version_major: 1 } content: "migration fixture text"',
    capture_output=True,
    check=True,
).stdout


def migrations():
    return sorted(
        (HERE.parent / "migrations").glob("*/up.sql"),
        key=lambda p: p.parent.name.split("_")[0].replace("-", ""),
    )


def apply(conn, paths):
    conn.executescript(
        "CREATE TABLE IF NOT EXISTS __diesel_schema_migrations(version VARCHAR(50) PRIMARY KEY NOT NULL, run_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP);"
    )
    for path in paths:
        version = path.parent.name.split("_")[0].replace("-", "")
        conn.executescript(
            "BEGIN;\n"
            + path.read_text()
            + f"\nINSERT INTO __diesel_schema_migrations(version) VALUES ('{version}'); COMMIT;"
        )


def populate(conn, early=False):
    conn.executescript(
        "DELETE FROM group_messages; DELETE FROM groups; DELETE FROM consent_records;"
    )
    groups = [
        (GROUP, 2, 1),
        ("44" * 16, 1, 1),
        ("66" * 16, 3, 1),
        ("77" * 16, 4, 1),
        ("88" * 16, 1, 4),
    ]
    for gid, kind, membership in groups:
        conn.execute(
            "INSERT INTO groups(id,created_at_ns,membership_state,installations_last_checked,added_by_inbox_id,conversation_type) VALUES (?,?,?,?,?,?)",
            (bytes.fromhex(gid), 1600000000000000001, membership, 0, OWNER, kind),
        )
    if not early:
        conn.execute(
            "UPDATE groups SET dm_id=?,message_disappear_from_ns=?,message_disappear_in_ns=? WHERE id=?",
            (
                f"dm:{OWNER}:{PEER}",
                1700000000000000000,
                60000000000,
                bytes.fromhex(GROUP),
            ),
        )
    for index, gid, expiry, kind in [
        (1, GROUP, None, 1),
        (2, GROUP, 1, 1),
        (3, GROUP, 9223372036854775807, 1),
        (4, GROUP, None, 2),
        (5, "66" * 16, None, 1),
        (6, "77" * 16, None, 1),
        (7, "88" * 16, None, 1),
        (8, "44" * 16, None, 1),
        (10, GROUP, None, 1),
    ]:
        cols = "id,group_id,decrypted_message_bytes,sent_at_ns,kind,sender_installation_id,sender_inbox_id,delivery_status"
        values = [
            bytes([index]) * 32,
            bytes.fromhex(gid),
            CONTENT,
            1500000000000000000 if index == 10 else 1700000000000000123,
            kind,
            bytes.fromhex("55" * 32),
            OWNER,
            2,
        ]
        if not early:
            cols += ",expire_at_ns,content_type,version_major,authority_id,originator_id,sequence_id"
            values += [expiry, 16, 99, "conflicting.example", 1, index]
        conn.execute(
            f"INSERT INTO group_messages({cols}) VALUES ({','.join('?' for _ in values)})",
            values,
        )
    if any(
        row[1] == "consented_at_ns"
        for row in conn.execute("PRAGMA table_info(consent_records)")
    ):
        conn.execute(
            "INSERT INTO consent_records(entity_type,state,entity,consented_at_ns) VALUES(2,1,?,1700000000000000009)",
            (PEER,),
        )
    else:
        conn.execute(
            "INSERT INTO consent_records(entity_type,state,entity) VALUES(2,1,?)",
            (PEER,),
        )
    if not early:
        conn.execute(
            "UPDATE __diesel_schema_migrations SET run_on='2020-01-01 00:00:00' WHERE version='20250717111748'"
        )
    conn.commit()


def cipher_library():
    prefix = Path(shutil.which("sqlcipher")).parent.parent
    candidates = [prefix / "lib/libsqlcipher.dylib", prefix / "lib/libsqlcipher.so"]
    lib = C.CDLL(str(next(p for p in candidates if p.exists())))
    lib.sqlite3_open.argtypes = [C.c_char_p, C.POINTER(C.c_void_p)]
    lib.sqlite3_exec.argtypes = [
        C.c_void_p,
        C.c_char_p,
        C.c_void_p,
        C.c_void_p,
        C.POINTER(C.c_char_p),
    ]
    lib.sqlite3_close.argtypes = [C.c_void_p]
    return lib


def encrypted_writer():
    lib = cipher_library()
    ptr = C.c_void_p()
    path = HERE / "encrypted.db3"
    assert lib.sqlite3_open(os.fsencode(path), C.byref(ptr)) == 0

    def sql(text):
        error = C.c_char_p()
        status = lib.sqlite3_exec(ptr, text.encode(), None, None, C.byref(error))
        if status:
            raise RuntimeError(status, error.value)

    sql(
        f"PRAGMA key=\"x'{KEY}'\"; PRAGMA cipher_plaintext_header_size=32; PRAGMA cipher_salt=\"x'{SALT}'\";"
    )
    with sqlite3.connect(HERE / "stable.db3") as plain:
        sql("\n".join(plain.iterdump()))
    sql("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0;")
    sql(
        f"INSERT INTO group_messages(id,group_id,decrypted_message_bytes,sent_at_ns,sender_installation_id,sender_inbox_id,delivery_status,authority_id,originator_id,sequence_id) VALUES(x'{'09' * 32}',x'{GROUP}',x'{CONTENT.hex()}',1700000000000000124,x'{'55' * 32}','{OWNER}',2,'xmtp.org',1,9);"
    )
    (HERE / "encrypted.db3.sqlcipher_salt").write_text(SALT)
    os._exit(0)  # Keep committed WAL after the fixture writer exits.


def consent_states_fixture():
    destination = HERE / "consent-states.db3"
    destination.unlink(missing_ok=True)
    shutil.copyfile(HERE / "stable.db3", destination)
    with sqlite3.connect(destination) as conn:
        conn.execute("DELETE FROM consent_records")
        conn.executemany(
            "INSERT INTO consent_records(entity_type,state,entity,consented_at_ns) VALUES(2,?,?,1700000000000000009)",
            [(state, f"{state + 2:02x}" * 32) for state in range(3)],
        )


def main():
    for name in [
        "stable.db3",
        "early.db3",
        "mobile-4.10.db3",
        "encrypted.db3",
        "encrypted.db3-wal",
        "encrypted.db3-shm",
        "encrypted.db3.sqlcipher_salt",
    ]:
        (HERE / name).unlink(missing_ok=True)
    shutil.copyfile(HERE / "metadata-seed.db3", HERE / "stable.db3")
    with sqlite3.connect(HERE / "stable.db3") as conn:
        populate(conn)
        conn.execute("PRAGMA wal_checkpoint(TRUNCATE)")
        conn.execute("PRAGMA journal_mode=DELETE")
    consent_states_fixture()
    paths = migrations()
    expiry = next(
        i for i, p in enumerate(paths) if "ADD COLUMN expire_at_ns" in p.read_text()
    )
    with sqlite3.connect(HERE / "early.db3") as conn:
        apply(conn, paths[:expiry])
        populate(conn, early=True)
    with sqlite3.connect(HERE / "mobile-4.10.db3") as conn:
        apply(conn, paths[:62])
        populate(conn)
        with sqlite3.connect(
            "file:" + str(HERE / "metadata-seed.db3") + "?immutable=1", uri=True
        ) as seed:
            conn.executemany(
                "INSERT OR REPLACE INTO openmls_key_value(key_bytes,value_bytes,version) VALUES(?,?,?)",
                seed.execute(
                    "SELECT key_bytes,value_bytes,version FROM openmls_key_value"
                ),
            )
        conn.commit()
    subprocess.run([sys.executable, __file__, "encrypted"], check=True)
    (HERE / "encrypted.db3-shm").unlink(missing_ok=True)
    expected = {
        "group_id": GROUP,
        "message_ids": ["01" * 32, "08" * 32, "0a" * 32],
        "message_bytes_hex": CONTENT.hex(),
        "consent_inbox": PEER,
        "stable_counts": [2, 3, 1],
        "consent_states": {"02" * 32: [0, 1], "03" * 32: [1, 2], "04" * 32: [2, 3]},
        "early_counts": [2, 5, 1],
        "encrypted_counts": [2, 4, 1],
        "wal_message_id": "09" * 32,
        "database_key_hex": KEY,
        "salt_hex": SALT,
        "early_migrations": expiry,
        "endpoint_migrations": len(paths),
    }
    (HERE / "expected.json").write_text(json.dumps(expected, indent=2) + "\n")
    files = sorted(
        p
        for p in HERE.iterdir()
        if p.name.endswith(
            (".db3", "-wal", ".sqlcipher_salt", ".json", ".rs", ".bincode")
        )
        and p.name != "hashes.json"
    )
    (HERE / "hashes.json").write_text(
        json.dumps(
            {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in files},
            indent=2,
        )
        + "\n"
    )
    print(json.dumps(expected))


if __name__ == "__main__":
    if len(sys.argv) > 1:
        encrypted_writer()
    else:
        main()
