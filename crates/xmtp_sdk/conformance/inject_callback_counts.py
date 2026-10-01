#!/usr/bin/env python3
"""Expose real handle counts in conformance copies of generated bindings."""

from pathlib import Path
import sys

FAMILIES = ("Signer", "PreAuthenticate", "CredentialSource", "EventListener", "LogSink")
language, filename = sys.argv[1:]
path = Path(filename)
source = path.read_text()
marker = "sdkConformanceCallbackHandleCounts"
if marker in source:
    raise SystemExit("callback counters are already installed")
for family in FAMILIES:
    if f"FfiConverterType{family}" not in source:
        raise SystemExit(f"missing generated callback converter: {family}")

if language == "typescript":
    entries = ",\n".join(
        f"    {family}: count(FfiConverterType{family})" for family in FAMILIES
    )
    source += (
        """
import { uniffiForeignFutureHandleCount as lifetimeFutureCount } from "@ubjs/core";
export function sdkConformanceCallbackHandleCounts(): Record<string, number> {
  function count(converter: object): number {
    const handles: unknown = Reflect.get(converter, "handleMap");
    if (typeof handles !== "object" || handles === null)
      throw new Error("generated callback handle map changed");
    const size: unknown = Reflect.get(handles, "size");
    if (typeof size !== "number") throw new Error("callback handle count is not numeric");
    return size;
  }
  return {
"""
        + entries
        + """,
    foreignFutures: lifetimeFutureCount(),
  };
}
"""
    )
elif language == "swift":
    old = """    var count: Int {
        get {
            map.count
        }
    }"""
    new = """    var count: Int {
        get {
            lock.withLock { map.count }
        }
    }"""
    if source.count(old) != 1:
        raise SystemExit("Swift handle count locking seam changed")
    source = source.replace(old, new)
    entries = ",\n".join(
        f'        "{family}": FfiConverterType{family}.handleMap.count'
        for family in FAMILIES
    )
    source += (
        """
public func sdkConformanceCallbackHandleCounts() -> [String: Int] {
    [
"""
        + entries
        + """,
        "foreignFutures": uniffiForeignFutureHandleCountXmtpSdk(),
    ]
}
"""
    )
elif language == "kotlin":
    entries = ",\n".join(
        f'    "{family}" to FfiConverterType{family}.handleMap.size'
        for family in FAMILIES
    )
    source += (
        """
fun sdkConformanceCallbackHandleCounts(): Map<String, Int> = mapOf(
"""
        + entries
        + """,
    "foreignFutures" to uniffiForeignFutureHandleCount(),
)
"""
    )
else:
    raise SystemExit(f"unsupported language: {language}")
path.write_text(source)
