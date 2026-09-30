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
        "                            try {\n"
        "                            EventStartHookForTest.beforeCallback?.invoke()\n"
        "                            if (!gate.begin()) return@withContext\n",
    )
    source = replace_once(
        source,
        "                                throw ListenerException.Failed()\n                            }\n",
        "                                throw ListenerException.Failed()\n                            }\n"
        "                            } finally { EventStartHookForTest.afterCallback?.invoke() }\n",
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
        "    func finish() async { await finished?() }\n"
        "    func run() async {\n"
        "        await hook?()\n"
        "    }\n"
        "}\n\n"
        "final class ListenerStartGate: @unchecked Sendable {\n",
    )
    source = replace_once(
        source,
        "        guard gate.begin() else { return }\n",
        "        await EventStartHookForTest.shared.run()\n"
        "        guard gate.begin() else { await EventStartHookForTest.shared.finish(); return }\n",
    )
    source = replace_once(
        source,
        "            throw ListenerError.Failed\n        }\n",
        "            await EventStartHookForTest.shared.finish()\n"
        "            throw ListenerError.Failed\n        }\n"
        "        await EventStartHookForTest.shared.finish()\n",
    )
elif language == "typescript":
    source = replace_once(
        source,
        "declare const process: { cwd(): string } | undefined;\n",
        "let eventStartHookForTest: (() => Promise<void>) | undefined;\n"
        "let eventFinishedHookForTest: (() => Promise<void>) | undefined;\n\n"
        "export function setEventStartHookForTest(hook?: () => Promise<void>, finished?: () => Promise<void>): void {\n"
        "  eventStartHookForTest = hook;\n"
        "  eventFinishedHookForTest = finished;\n"
        "}\n\n"
        "declare const process: { cwd(): string } | undefined;\n",
    )
    source = replace_once(
        source,
        "          if (gate.stopped) return;\n",
        "          try {\n"
        "          if (eventStartHookForTest) await eventStartHookForTest();\n"
        "          if (gate.stopped) return;\n",
    )
    source = replace_once(
        source,
        "            throw new ListenerError.Failed();\n          }\n",
        "            throw new ListenerError.Failed();\n          }\n"
        "          } finally { await eventFinishedHookForTest?.(); }\n",
    )
else:
    raise SystemExit(f"unknown conformance language: {language}")
path.write_text(source)
