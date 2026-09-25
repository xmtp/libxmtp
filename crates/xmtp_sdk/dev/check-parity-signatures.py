#!/usr/bin/env python3
"""Compare the generated Node and browser method and record declarations."""

from pathlib import Path
import re
import sys


ROOT = Path(__file__).resolve().parents[3]
GENERATED = ROOT / "target/sdk-generated"
NODE = (GENERATED / "typescript-napi/xmtp_sdk.ts").read_text()
BROWSER = (GENERATED / "typescript-wasm/xmtp_sdk.ts").read_text()

REMOVED = {
    "ArchivesLike": {"exportToFile", "importFromFile", "metadataFromFile"},
    "StorageLike": {"delete_", "reconnect"},
    "ClientLike": {"disableNotifications", "enableNotifications", "notificationState"},
}
INTERFACES = (
    "BackendLike",
    "MessageReaderLike",
    "GroupLike",
    "DmLike",
    "ArchivesLike",
    "ConversationsLike",
    "DiagnosticsLike",
    "PreferencesLike",
    "StorageLike",
    "SignatureRequestLike",
    "ClientLike",
)
RECORDS = ("ClientOptions", "EncodedContent", "MessageData", "StorageOptions")


def body(source: str, declaration: str) -> str:
    match = re.search(
        rf"^export (?:interface|type) {declaration}\b[^{{]*\{{", source, re.M
    )
    if match is None:
        raise ValueError(f"missing {declaration}")
    start = match.end()
    depth = 1
    for pos in range(start, len(source)):
        if source[pos] == "{":
            depth += 1
        elif source[pos] == "}":
            depth -= 1
            if depth == 0:
                return source[start:pos]
    raise ValueError(f"unterminated {declaration}")


def declarations(source: str, name: str, method: bool) -> dict[str, str]:
    result = {}
    for line in body(source, name).splitlines():
        line = line.strip().rstrip(",;")
        match = re.match(r"^([A-Za-z_][A-Za-z_0-9]*)\??:", line)
        if match is None and method:
            match = re.match(r"^([A-Za-z_][A-Za-z_0-9]*)\(", line)
        if match is None:
            continue
        key = match.group(1)
        result[key] = re.sub(r"\s+", "", line)
    return result


def check(name: str, method: bool) -> None:
    native = declarations(NODE, name, method)
    web = declarations(BROWSER, name, method)
    for key in REMOVED.get(name, set()):
        if key not in native:
            raise ValueError(f"{name}.{key}: expected native method is missing")
        del native[key]
    if name == "StorageOptions":
        if "encryptionKey" not in native:
            raise ValueError("StorageOptions.encryptionKey is missing on Node")
        del native["encryptionKey"]
    if native != web:
        for key in sorted(native.keys() | web.keys()):
            if native.get(key) != web.get(key):
                raise ValueError(f"{name}.{key}: Node and browser declarations differ")


try:
    for interface in INTERFACES:
        check(interface, True)
    for record in RECORDS:
        check(record, False)
except ValueError as error:
    print(error, file=sys.stderr)
    raise SystemExit(1) from error
print("Node and browser parameter, return, and record declarations match")
