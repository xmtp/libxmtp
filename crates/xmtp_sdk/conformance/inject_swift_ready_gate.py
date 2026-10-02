"""Pause real constructor results after READY in the Swift conformance copy."""

from pathlib import Path
import re
import sys

path = Path(sys.argv[1])
source = path.read_text()
helper = "fileprivate func uniffiRustCallAsync<F, T>(\n"
assert source.count(helper) == 1, "Swift caller helper changed"
if "    readyHook: (() async -> Void)? = nil,\n" not in source:
    source = source.replace(
        helper, helper + "    readyHook: (() async -> Void)? = nil,\n"
    )
ready = "        } while pollResult != UNIFFI_RUST_FUTURE_POLL_READY\n"
assert source.count(ready) == 1, "Swift READY boundary changed"
if "        await readyHook?()\n" not in source:
    source = source.replace(ready, ready + "        await readyHook?()\n")
if "    afterLiftHook: (() async -> Void)? = nil,\n" not in source:
    source = source.replace(helper, helper + "    afterLiftHook: (() async -> Void)? = nil,\n")
if "        await afterLiftHook?()\n" not in source:
    lift = "        let lifted = try liftFunc(value)\n"
    assert source.count(lift) == 1, "Swift lift boundary changed"
    source = source.replace(lift, lift + "        await afterLiftHook?()\n")
for method in ["create_ready", "build_ready"]:
    pattern = (
        r"(await uniffiRustCallAsync\(\n)(\s*rustFutureFunc: \{\n"
        r"\s*uniffi_xmtp_sdk_fn_method_sdkconformanceconstructorprobe_"
        + method
        + r"\()"
    )
    source, count = re.subn(
        pattern,
        r"\1            readyHook: { await sdkConformanceSwiftReadyGate.hold() },\n            afterLiftHook: { await sdkConformanceSwiftLiftedGate.hold() },\n\2",
        source,
    )
    assert count == 1, f"Swift {method} call site changed"
    start = source.index("uniffi_xmtp_sdk_fn_method_sdkconformanceconstructorprobe_" + method + "(")
    end = source.index("errorHandler: FfiConverterTypeXmtpError_lift", start)
    block = source[start:end]
    for kind, args in [("poll", "h, cb, data"), ("complete", "h, status"), ("cancel", "h"), ("free", "h")]:
        old = f"{kind}Func: ffi_xmtp_sdk_rust_future_{kind}_u64,"
        assert block.count(old) == 1, f"Swift constructor {kind} call changed"
        result = "return " if kind == "complete" else ""
        block = block.replace(old, f'{kind}Func: {{ {args} in sdkConformanceSwiftConstructorCalls.record("{kind}"); {result}ffi_xmtp_sdk_rust_future_{kind}_u64({args}) }},')
    source = source[:start] + block + source[end:]
source += """

public actor SdkConformanceSwiftReadyGate {
    private var entered = false
    private var released = false
    private var waiters: [CheckedContinuation<Void, Never>] = []

    public func reset() {
        precondition(waiters.isEmpty)
        entered = false
        released = false
    }

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

public let sdkConformanceSwiftReadyGate = SdkConformanceSwiftReadyGate()
public let sdkConformanceSwiftLiftedGate = SdkConformanceSwiftReadyGate()

public final class SdkConformanceSwiftConstructorCalls: @unchecked Sendable {
    private let lock = NSLock()
    private var counts: [String: Int] = [:]
    public func reset() { lock.lock(); counts = [:]; lock.unlock() }
    public func record(_ key: String) { lock.lock(); counts[key, default: 0] += 1; lock.unlock() }
    public func snapshot() -> [String: Int] { lock.lock(); defer { lock.unlock() }; return counts }
}
public let sdkConformanceSwiftConstructorCalls = SdkConformanceSwiftConstructorCalls()
"""
path.write_text(source)
