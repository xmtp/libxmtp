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
for method in ["create_ready", "build_ready"]:
    pattern = (
        r"(await uniffiRustCallAsync\(\n)(\s*rustFutureFunc: \{\n"
        r"\s*uniffi_xmtp_sdk_fn_method_sdkconformanceconstructorprobe_"
        + method
        + r"\()"
    )
    source, count = re.subn(
        pattern,
        r"\1            readyHook: { await sdkConformanceSwiftReadyGate.hold() },\n\2",
        source,
    )
    assert count == 1, f"Swift {method} call site changed"
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
"""
path.write_text(source)
