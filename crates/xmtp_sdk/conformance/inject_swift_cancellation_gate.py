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
    + "    readyHook: (() async -> Void)? = nil,\n    liftHook: (() async -> Void)? = nil,\n",
)
ready = "        } while pollResult != UNIFFI_RUST_FUTURE_POLL_READY\n"
assert source.count(ready) == 1, "Swift READY boundary changed"
source = source.replace(ready, ready + "        await readyHook?()\n")
lift = "        let lifted = try liftFunc(value)"
assert source.count(lift) == 1, "Swift lift boundary changed"
source = source.replace(lift, "        await liftHook?()\n" + lift)
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

public final class SdkConformanceSwiftReaderCalls: @unchecked Sendable {
    private let lock = NSLock()
    private var counts: [String: Int] = [:]
    public func reset() { lock.lock(); counts = [:]; lock.unlock() }
    public func record(_ key: String) { lock.lock(); counts[key, default: 0] += 1; lock.unlock() }
    public func snapshot() -> [String: Int] { lock.lock(); defer { lock.unlock() }; return counts }
}
public let sdkConformanceSwiftReaderCalls = SdkConformanceSwiftReaderCalls()
"""
path.write_text(source)
