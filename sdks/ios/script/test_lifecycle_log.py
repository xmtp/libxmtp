#!/usr/bin/env python3
"""Check the error text sent by the actual Swift lifecycle manager."""

import argparse
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[3]
parser = argparse.ArgumentParser()
parser.add_argument(
    "--source",
    type=Path,
    default=ROOT / "apps/xmtp_sdk_bindgen/runtime/swift/AppleLifecycle.swift",
)
args = parser.parse_args()
source = args.source.read_text().replace("import os\n", "")
stubs = """
public final class SDKClient {}
func suspendStreams() async throws {}
func resumeStreams() async throws {}
enum OSLogType { case error }
struct OSLog { static let `default` = OSLog() }
final class LogCalls: @unchecked Sendable {
    static let shared = LogCalls()
    private let lock = NSLock()
    private var text = ""
    func record(_ value: String) {
        lock.lock(); defer { lock.unlock() }; text += value
    }
    func value() -> String {
        lock.lock(); defer { lock.unlock() }; return text
    }
}
func os_log(_ format: StaticString, log: OSLog, type: OSLogType, _ args: CVarArg...) {
    let text = String(describing: format).replacingOccurrences(of: "%{public}@", with: "%@")
    LogCalls.shared.record(String(format: text, arguments: args))
}
struct BackendFailure: LocalizedError {
    var errorDescription: String? { "backend rejected credential-lifecycle-secret" }
}
"""
checks = """
@main struct Checks {
    static func main() async {
        let manager = StreamLifecycleManager(
            suspend: {}, resume: { throw BackendFailure() }
        )
        await manager.setDesired(live: false)?.value
        await manager.setDesired(live: true)?.value
        let text = LogCalls.shared.value()
        guard text.contains("resume") else { fatalError("failed resume did not log") }
        guard !text.contains("credential-lifecycle-secret") else {
            fatalError("lifecycle log exposed backend error text")
        }
        print("PASS: lifecycle failure log omits backend error text")
    }
}
"""
with tempfile.TemporaryDirectory(prefix="xmtp-lifecycle-log-") as directory:
    path = Path(directory)
    swift = path / "Checks.swift"
    swift.write_text("import Foundation\n" + stubs + source + checks)
    binary = path / "checks"
    subprocess.run(
        ["swiftc", "-parse-as-library", str(swift), "-o", str(binary)], check=True
    )
    subprocess.run([str(binary)], check=True)
