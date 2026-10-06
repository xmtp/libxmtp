#!/usr/bin/env python3
"""Add a callback start hook to a conformance-only runtime copy."""

from pathlib import Path
import sys


def replace_once(source: str, old: str, new: str) -> str:
    if source.count(old) != 1:
        raise SystemExit(f"event start seam changed: expected one match for {old!r}")
    return source.replace(old, new, 1)


language, filename = sys.argv[1:]
path = Path(filename)
source = path.read_text()
if language == "kotlin":
    source = replace_once(
        source,
        "internal class ListenerStartGate {\n",
        "internal object EventStartHookForTest {\n"
        "    @Volatile var beforeCallback: (suspend () -> Unit)? = null\n"
        "    @Volatile var afterCallback: (suspend () -> Unit)? = null\n"
        "}\n\n"
        "internal class ListenerStartGate {\n",
    )
    source = replace_once(
        source,
        "                            if (!gate.begin()) return@withContext\n",
        "                            val callbackFinished = EventStartHookForTest.afterCallback\n"
        "                            try {\n"
        "                            EventStartHookForTest.beforeCallback?.invoke()\n"
        "                            if (!gate.begin()) return@withContext\n",
    )
    source = replace_once(
        source,
        "                                throw ListenerException.Failed()\n                            }\n",
        "                                throw ListenerException.Failed()\n                            }\n"
        "                            } finally { callbackFinished?.invoke() }\n",
    )
elif language == "swift":
    source = replace_once(
        source,
        "final class ListenerStartGate: @unchecked Sendable {\n",
        "actor EventStartHookForTest {\n"
        "    static let shared = EventStartHookForTest()\n"
        "    private var hook: (@Sendable () async -> Void)?\n"
        "    private var finished: (@Sendable () async -> Void)?\n\n"
        "    func set(_ hook: (@Sendable () async -> Void)?, finished: (@Sendable () async -> Void)? = nil) {\n"
        "        self.hook = hook\n"
        "        self.finished = finished\n"
        "    }\n\n"
        "    func run() async -> (@Sendable () async -> Void)? {\n"
        "        let callbackFinished = finished\n"
        "        await hook?()\n"
        "        return callbackFinished\n"
        "    }\n"
        "}\n\n"
        "final class ListenerStartGate: @unchecked Sendable {\n",
    )
    source = replace_once(
        source,
        "        guard gate.begin() else { return }\n",
        "        let callbackFinished = await EventStartHookForTest.shared.run()\n"
        "        guard gate.begin() else { await callbackFinished?(); return }\n",
    )
    source = replace_once(
        source,
        "            throw ListenerError.Failed\n        }\n",
        "            await callbackFinished?()\n"
        "            throw ListenerError.Failed\n        }\n"
        "        await callbackFinished?()\n",
    )
else:
    raise SystemExit(f"unknown conformance language: {language}")
path.write_text(source)
