#!/usr/bin/env python3
"""Check the actual Swift listener start gates after client close."""

import argparse
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[3]
parser = argparse.ArgumentParser()
parser.add_argument(
    "--source",
    type=Path,
    default=ROOT / "apps/xmtp_sdk_bindgen/runtime/swift/events/SDKEvents.swift",
)
args = parser.parse_args()
source = args.source.read_text().split("public extension SDKClient {", 1)[0]
types = """
typealias ListenerId = UInt64
struct ClientEvent: Sendable {}
protocol EventListener { func onEvent(event: ClientEvent) async throws }
enum ListenerError: Error { case Failed }
actor Calls {
    var count = 0
    func record() { count += 1 }
}
"""
checks = """
@main struct Checks {
    static func main() async throws {
        let calls = Calls()
        let registry = ListenerGates()
        registry.stopAll()
        let late = ListenerStartGate()
        registry.addPending(late)
        registry.registered(1, gate: late)
        let listener = ClosureEventListener({ _ in await calls.record() }, gate: late)
        try await listener.onEvent(event: ClientEvent())
        guard await calls.count == 0 else {
            fatalError("callback admitted after client close")
        }
        let lateRegistration = ListenerStartGate()
        registry.registered(2, gate: lateRegistration)
        guard !lateRegistration.begin() else {
            fatalError("registration admitted after client close")
        }
        let pendingRegistry = ListenerGates()
        let pending = ListenerStartGate()
        pendingRegistry.addPending(pending)
        pendingRegistry.stopAll()
        pendingRegistry.registered(3, gate: pending)
        guard !pending.begin() else { fatalError("pending listener survived close") }
        registry.stopAll()
        print("PASS: closed registry rejects pending and registered callbacks")
    }
}
"""
with tempfile.TemporaryDirectory(prefix="xmtp-listener-gates-") as directory:
    path = Path(directory)
    swift = path / "Checks.swift"
    swift.write_text("import Foundation\n" + types + source + checks)
    binary = path / "checks"
    subprocess.run(
        ["swiftc", "-parse-as-library", str(swift), "-o", str(binary)], check=True
    )
    subprocess.run([str(binary)], check=True)
