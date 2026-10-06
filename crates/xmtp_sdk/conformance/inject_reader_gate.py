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
if language == "swift":
    source = replace_once(
        source,
        "    let raw: Client\n",
        "    let raw: Client\n\n"
        "    nonisolated(unsafe) static var readerOpenedForTest: (@Sendable (MessageReader) async -> Void)?\n"
        "    nonisolated(unsafe) static var conversationReaderOpeningForTest: (@Sendable () async -> Void)?\n"
        "    nonisolated(unsafe) static var conversationReaderOpenedForTest: (@Sendable (ConversationReader) async -> Void)?\n",
    )
    path.write_text(source)
    readers = path.parent / "streams" / "Readers.swift"
    message, marker, conversation = readers.read_text().partition(
        "func makeSDKConversationStream("
    )
    if not marker:
        raise SystemExit("reader gate seam changed: conversation adapter missing")
    message = replace_once(
        message,
        "        let reader = try await open()\n",
        "        let reader = try await open()\n"
        "        await SDKClient.readerOpenedForTest?(reader)\n",
    )
    conversation = replace_once(
        conversation,
        "        let reader = try await open()\n",
        "        await SDKClient.conversationReaderOpeningForTest?()\n"
        "        let reader = try await open()\n"
        "        await SDKClient.conversationReaderOpenedForTest?(reader)\n",
    )
    readers_source = message + marker + conversation
    readers_source = replace_once(
        readers_source,
        "private final class StreamHandle<Value>: @unchecked Sendable {",
        "final class StreamHandle<Value>: @unchecked Sendable {",
    )
    sequence, marker, iterator = readers_source.partition(
        "/// The loop releases this object"
    )
    if not marker:
        raise SystemExit("reader gate seam changed: iterator marker missing")
    sequence = replace_once(
        sequence,
        "    fileprivate init(\n        open: @escaping @Sendable () async throws -> StreamHandle<Value>,\n"
        "        onClose: (@Sendable (SDKStreamCloseReason) throws -> Void)?,",
        "    init(\n        open: @escaping @Sendable () async throws -> StreamHandle<Value>,\n"
        "        onClose: (@Sendable (SDKStreamCloseReason) throws -> Void)?,",
    )
    readers_source = sequence + marker + iterator
    readers.write_text(readers_source)
else:
    raise SystemExit(f"unknown conformance language: {language}")
