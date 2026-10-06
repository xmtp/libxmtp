"""The deterministic message dataset that every host seeds, reads and streams."""

import hashlib
import json

ROWS = 1000


def canonical(value):
    return json.dumps(
        value, sort_keys=True, separators=(",", ":"), ensure_ascii=True
    ).encode()


def digest(value):
    return hashlib.sha256(canonical(value)).hexdigest()


def dataset():
    """Every four rows hold a text, a reply to that text, an attachment, and a
    text with one reaction. Keys are local; hosts map them to real IDs."""
    messages = []
    for index in range(ROWS):
        kind = index % 4
        messages.append(
            {
                "key": str(index),
                "text": None if kind == 2 else f"bench message {index:05d}",
                "reply_to": str(index - 1) if kind == 1 else None,
                "parent_text": f"bench message {index - 1:05d}" if kind == 1 else None,
                "reactions": [{"content": "+1", "schema": "unicode", "action": "added"}]
                if kind == 3
                else [],
                "attachment": {
                    "filename": f"fixture-{index:05d}.bin",
                    "mime_type": "application/octet-stream",
                    "bytes_hex": bytes((index + j) % 256 for j in range(128)).hex(),
                }
                if kind == 2
                else None,
            }
        )
    return {"schema": 3, "messages": messages}


def stream_events(fixture):
    """Published stream events: each primary message and each reaction."""
    return sum(1 + len(row["reactions"]) for row in fixture["messages"])
