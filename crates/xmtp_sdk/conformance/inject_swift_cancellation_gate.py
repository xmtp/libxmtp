"""Count real reader native calls and pause a READY result in conformance copies."""

from pathlib import Path
import re
import sys

path = Path(sys.argv[1])
source = path.read_text()
helper = "fileprivate func uniffiRustCallAsync<F, T>(\n"
assert source.count(helper) == 1, "Swift caller helper changed"
source = source.replace(
    helper,
    helper
    + "    readyHook: (() async -> Void)? = nil,\n    liftHook: (() async -> Void)? = nil,\n    afterLiftHook: (() async -> Void)? = nil,\n",
)
ready = "        } while pollResult != UNIFFI_RUST_FUTURE_POLL_READY\n"
assert source.count(ready) == 1, "Swift READY boundary changed"
source = source.replace(ready, ready + "        await readyHook?()\n")
lift = "        let lifted = try liftFunc(value)"
assert source.count(lift) == 1, "Swift lift boundary changed"
source = source.replace(lift, "        await liftHook?()\n" + lift + "\n        await afterLiftHook?()")
pattern = r"(await uniffiRustCallAsync\(\n)(\s*rustFutureFunc: \{\n\s*uniffi_xmtp_sdk_fn_method_messagereader_next\()"
source, count = re.subn(
    pattern,
    r"\1            readyHook: { await sdkConformanceSwiftReaderGate.hold() },\n            liftHook: { await sdkConformanceSwiftReaderLiftGate.hold() },\n\2",
    source,
)
assert count == 1, "Swift reader call site changed"
start = source.index("uniffi_xmtp_sdk_fn_method_messagereader_next(")
end = source.index("errorHandler: FfiConverterTypeXmtpError_lift", start)
block = source[start:end]
for kind, args in [
    ("poll", "h, cb, data"),
    ("complete", "h, status"),
    ("cancel", "h"),
    ("free", "h"),
]:
    old = f"{kind}Func: ffi_xmtp_sdk_rust_future_{kind}_rust_buffer,"
    assert block.count(old) == 1, f"Swift reader {kind} call changed"
    result = "return " if kind == "complete" else ""
    block = block.replace(
        old,
        f'{kind}Func: {{ {args} in sdkConformanceSwiftReaderCalls.record("{kind}"); {result}ffi_xmtp_sdk_rust_future_{kind}_rust_buffer({args}) }},',
    )
source = source[:start] + block + source[end:]
final = "        if let eventReadResult {"
assert source.count(final) == 1, "Swift final event handoff changed"
source = source.replace(final, "        await finalEventHook?()\n" + final)
argument = "\n    eventReadResult:"
assert source.count(argument) == 1, "Swift event result argument changed"
source = source.replace(argument, "\n    finalEventHook: (() async -> Void)? = nil," + argument)
event_call = "            eventReadResult:"
assert source.count(event_call) == 1, "Swift event handoff call changed"
source = source.replace(event_call, "            finalEventHook: { await sdkConformanceSwiftEventFinalGate.hold() },\n" + event_call)

pattern = r"(await uniffiRustCallAsync\(\n)(\s*(?:eventReadResult:[^\n]*\n\s*endCancelledEventRead:[^\n]*\n\s*cancelEventRead:[^\n]*\n)?\s*rustFutureFunc: \{\n\s*uniffi_xmtp_sdk_fn_method_eventreader_next\()"
source, count = re.subn(
    pattern,
    r"\1            readyHook: { await sdkConformanceSwiftEventGate.hold() },\n            afterLiftHook: { await sdkConformanceSwiftEventLiftGate.hold() },\n\2",
    source,
)
assert count == 1, "Swift event reader call site changed"
start = source.index("uniffi_xmtp_sdk_fn_method_eventreader_next(")
end = source.index("errorHandler: FfiConverterTypeXmtpError_lift", start)
block = source[start:end]
for kind, args in [("poll", "h, cb, data"), ("complete", "h, status"), ("cancel", "h"), ("free", "h")]:
    old = f"{kind}Func: ffi_xmtp_sdk_rust_future_{kind}_rust_buffer,"
    assert block.count(old) == 1, f"Swift event {kind} call changed"
    result = "return " if kind == "complete" else ""
    block = block.replace(old, f'{kind}Func: {{ {args} in sdkConformanceSwiftEventCalls.record("{kind}"); {result}ffi_xmtp_sdk_rust_future_{kind}_rust_buffer({args}) }},')
source = source[:start] + block + source[end:]

start = source.index("uniffi_xmtp_sdk_fn_method_eventreader_end(")
end = source.index("errorHandler: FfiConverterTypeXmtpError_lift", start)
block = source[start:end]
for kind, args in [("poll", "h, cb, data"), ("complete", "h, status"), ("cancel", "h"), ("free", "h")]:
    old = f"{kind}Func: ffi_xmtp_sdk_rust_future_{kind}_void,"
    assert block.count(old) == 1, f"Swift event end {kind} call changed"
    result = "return " if kind == "complete" else ""
    block = block.replace(old, f'{kind}Func: {{ {args} in sdkConformanceSwiftEventEndCalls.record("{kind}"); {result}ffi_xmtp_sdk_rust_future_{kind}_void({args}) }},')
source = source[:start] + block + source[end:]

for anchor, gate in [
    ("    sdkEventReadGates.stopAll()\n", "sdkConformanceSwiftClientEndGate"),
    ("    if fileBacked { sdkEventReadGates?.stopAll() }\n", "sdkConformanceSwiftStorageDeleteGate"),
]:
    assert source.count(anchor) == 1, "Swift close admission changed"
    source = source.replace(anchor, anchor + f"    await {gate}.hold()\n", 1)

for method, counts in [
    ("client_end", "sdkConformanceSwiftClientEndCalls"),
    ("storage_delete", "sdkConformanceSwiftStorageDeleteCalls"),
]:
    start = source.index(f"uniffi_xmtp_sdk_fn_method_{method}(")
    end = source.index("errorHandler: FfiConverterTypeXmtpError_lift", start)
    block = source[start:end]
    for kind, args in [("poll", "h, cb, data"), ("complete", "h, status"), ("cancel", "h"), ("free", "h")]:
        old = f"{kind}Func: ffi_xmtp_sdk_rust_future_{kind}_void,"
        assert block.count(old) == 1, f"Swift {method} {kind} call changed"
        result = "return " if kind == "complete" else ""
        block = block.replace(old, f'{kind}Func: {{ {args} in {counts}.record("{kind}"); {result}ffi_xmtp_sdk_rust_future_{kind}_void({args}) }},')
    source = source[:start] + block + source[end:]

source += """

public actor SdkConformanceSwiftReaderGate {
    private var entered = false
    private var released = true
    private var waiters: [CheckedContinuation<Void, Never>] = []
    public func reset() { precondition(waiters.isEmpty); entered = false; released = false }
    public func hold() async {
        entered = true
        if released { return }
        await withCheckedContinuation { waiters.append($0) }
    }
    public func didEnter() -> Bool { entered }
    public func release() {
        released = true
        let pending = waiters
        waiters.removeAll()
        for waiter in pending { waiter.resume() }
    }
}
public let sdkConformanceSwiftReaderGate = SdkConformanceSwiftReaderGate()
public let sdkConformanceSwiftReaderLiftGate = SdkConformanceSwiftReaderGate()
public let sdkConformanceSwiftEventGate = SdkConformanceSwiftReaderGate()
public let sdkConformanceSwiftEventLiftGate = SdkConformanceSwiftReaderGate()
public let sdkConformanceSwiftEventFinalGate = SdkConformanceSwiftReaderGate()
public let sdkConformanceSwiftClientEndGate = SdkConformanceSwiftReaderGate()
public let sdkConformanceSwiftStorageDeleteGate = SdkConformanceSwiftReaderGate()

public final class SdkConformanceSwiftReaderCalls: @unchecked Sendable {
    private let lock = NSLock()
    private var counts: [String: Int] = [:]
    public func reset() { lock.lock(); counts = [:]; lock.unlock() }
    public func record(_ key: String) { lock.lock(); counts[key, default: 0] += 1; lock.unlock() }
    public func snapshot() -> [String: Int] { lock.lock(); defer { lock.unlock() }; return counts }
}
public let sdkConformanceSwiftReaderCalls = SdkConformanceSwiftReaderCalls()
public let sdkConformanceSwiftEventCalls = SdkConformanceSwiftReaderCalls()
public let sdkConformanceSwiftEventEndCalls = SdkConformanceSwiftReaderCalls()
public let sdkConformanceSwiftClientEndCalls = SdkConformanceSwiftReaderCalls()
public let sdkConformanceSwiftStorageDeleteCalls = SdkConformanceSwiftReaderCalls()
"""
path.write_text(source)
