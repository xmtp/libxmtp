"""One deterministic semantic dataset for every installed package adapter."""

import hashlib
import json


def canonical(value):
    return json.dumps(
        value, sort_keys=True, separators=(",", ":"), ensure_ascii=True
    ).encode()


def digest(value):
    return hashlib.sha256(canonical(value)).hexdigest()


def dataset():
    messages = []
    for index in range(10000):
        # IDs are local fixture keys. Adapters map them to real published IDs.
        messages.append(
            {
                "key": str(index),
                "text": None if index % 4 == 2 else f"cutover message {index:05d}",
                "reply_to": str(index - 1) if index % 4 == 1 else None,
                "parent_text": f"cutover message {index - 1:05d}"
                if index % 4 == 1
                else None,
                "reactions": [{"content": "+1", "schema": "unicode", "action": "added"}]
                if index % 4 == 3
                else [],
                "attachment": {
                    "filename": f"fixture-{index:05d}.bin",
                    "mime_type": "application/octet-stream",
                    "bytes_hex": bytes((index + j) % 256 for j in range(128)).hex(),
                }
                if index % 4 == 2
                else None,
            }
        )
    return {
        "schema": 1,
        "seed": "xmtp-cutover-v1",
        "messages": messages,
        "page_keys": [str(i) for i in range(1000)],
        "stream_keys": [str(i) for i in range(10000)],
        "enrichment": ["decoded_content", "reply_parent", "reactions", "attachments"],
        "callback_delay_ms": 25,
    }


def expected_observation(fixture, workload):
    if workload in {"page", "stream", "mobile_record"}:
        count = 1000 if workload != "stream" else 10000
        rows = fixture["messages"][:count]
        return {"count": count, "semantic_sha256": digest(rows)}
    return {
        "count": 1,
        "semantic_sha256": digest({"workload": workload, "completed": True}),
    }
