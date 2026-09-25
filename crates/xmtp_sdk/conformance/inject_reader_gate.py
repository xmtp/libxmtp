#!/usr/bin/env python3
"""Add a reader-open gate to a conformance-only copy of the host runtime."""

from pathlib import Path
import sys


def replace_once(source: str, old: str, new: str) -> str:
    if source.count(old) != 1:
        raise SystemExit(f"reader gate seam changed: expected one match for {old!r}")
    return source.replace(old, new, 1)


language, filename = sys.argv[1:]
path = Path(filename)
source = path.read_text()
if language == "kotlin":
    source = replace_once(
        source,
        "    companion object {\n",
        "    companion object {\n"
        "        @Volatile\n"
        "        internal var readerOpenedForTest: (suspend (MessageReader) -> Unit)? = null\n\n",
    )
    source = replace_once(
        source,
        "            open = { group.messageReader() },",
        "            open = { group.messageReader().also { readerOpenedForTest?.invoke(it) } },",
    )
elif language == "swift":
    source = replace_once(
        source,
        "    public let raw: Client\n",
        "    public let raw: Client\n\n"
        "    nonisolated(unsafe) static var readerOpenedForTest: (@Sendable (MessageReader) async -> Void)?\n",
    )
    source = replace_once(
        source,
        "        let reader = try await group.messageReader()\n",
        "        let reader = try await group.messageReader()\n"
        "        await Self.readerOpenedForTest?(reader)\n",
    )
else:
    raise SystemExit(f"unknown conformance language: {language}")
path.write_text(source)
