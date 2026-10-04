#!/usr/bin/env python3
"""Check overlapping reads against the maintained Swift iterator source."""

import argparse
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[3]
parser = argparse.ArgumentParser()
parser.add_argument(
    "--source",
    type=Path,
    default=ROOT / "apps/xmtp_sdk_bindgen/runtime/swift/streams/Readers.swift",
)
args = parser.parse_args()
source = args.source.read_text().split("func makeSDKMessageStream(", 1)[0]
stubs = """
public struct Message {}
public struct Conversation {}
public final class SDKClient: @unchecked Sendable { let raw: Void = () }
public enum ConnectionState { case connected, closed }
public enum ErrorCategory { case stream }
public struct ErrorDetails {
    let code: String
    let category: ErrorCategory
    let retryable: Bool
    let message: String?
}
public enum XmtpError: Error { case ConsumerOwned(ErrorDetails) }
"""
checks = """
actor HeldReader {
    private var calls = 0
    private var ends = 0
    private var entered = false
    private var enteredWaiter: CheckedContinuation<Void, Never>?
    private var release: CheckedContinuation<Void, Never>?

    func next() async -> Int? {
        calls += 1
        if calls == 1 {
            entered = true
            enteredWaiter?.resume()
            enteredWaiter = nil
            await withCheckedContinuation { release = $0 }
            return 1
        }
        return calls == 2 ? 2 : nil
    }

    func waitForRead() async {
        if entered { return }
        await withCheckedContinuation { enteredWaiter = $0 }
    }

    func releaseRead() { release?.resume(); release = nil }
    func end() { ends += 1; release?.resume(); release = nil }
    func counts() -> (Int, Int) { (calls, ends) }
}

final class CloseCount: @unchecked Sendable {
    private let lock = NSLock()
    private var count = 0
    func increment() { lock.lock(); count += 1; lock.unlock() }
    func value() -> Int { lock.lock(); defer { lock.unlock() }; return count }
}

enum ReadFailure: Error { case failed }

func checkReleasedAfterFailure() async throws {
    let owner = SDKClient()
    let stream = SDKReaderStream<Int>(open: {
        StreamHandle(owner: owner, next: { throw ReadFailure.failed }, end: {},
                     connectionState: { .connected },
                     connectionStateChanged: { _ in .closed })
    }, onClose: nil, onConnectionStateChange: nil)
    let iterator = stream.makeAsyncIterator()
    do { _ = try await iterator.next(); fatalError("read failure did not throw") }
    catch ReadFailure.failed {}
    do { _ = try await iterator.next(); fatalError("failed iterator did not close") }
    catch is CancellationError {}
}

func checkReleasedAfterCancellation() async throws {
    let owner = SDKClient()
    let reader = HeldReader()
    let stream = SDKReaderStream<Int>(open: {
        StreamHandle(owner: owner, next: { await reader.next() },
                     end: { await reader.end() },
                     connectionState: { .connected },
                     connectionStateChanged: { _ in .closed })
    }, onClose: nil, onConnectionStateChange: nil)
    let iterator = stream.makeAsyncIterator()
    let first = Task { try await iterator.next() }
    await reader.waitForRead()
    first.cancel()
    _ = try? await first.value
    do { _ = try await iterator.next(); fatalError("cancelled iterator did not close") }
    catch is CancellationError {}
}

@main struct Checks {
    static func main() async throws {
        let reader = HeldReader()
        let closed = CloseCount()
        let owner = SDKClient()
        let stream = SDKReaderStream<Int>(open: {
            StreamHandle(owner: owner, next: { await reader.next() },
                         end: { await reader.end() },
                         connectionState: { .connected },
                         connectionStateChanged: { _ in .closed })
        }, onClose: { _ in closed.increment() }, onConnectionStateChange: nil)
        let iterator = stream.makeAsyncIterator()
        let first = Task { try await iterator.next() }
        await reader.waitForRead()
        var rejected = false
        do { _ = try await iterator.next() }
        catch let XmtpError.ConsumerOwned(details) {
            rejected = details.code == "ConsumerOwned" && !details.retryable
        }
        let beforeRelease = await reader.counts()
        let closeBeforeRelease = closed.value()
        await reader.releaseRead()
        let firstValue = try await first.value
        guard rejected else { fatalError("overlapping next did not reject") }
        guard beforeRelease.0 == 1, beforeRelease.1 == 0, closeBeforeRelease == 0 else {
            fatalError("rejected read advanced or ended the active reader")
        }
        guard firstValue == 1, try await iterator.next() == 2 else {
            fatalError("read ownership was not released after delivery")
        }
        guard try await iterator.next() == nil else { fatalError("stream did not end") }
        let afterEnd = await reader.counts()
        guard afterEnd.1 == 1, closed.value() == 1 else {
            fatalError("reader teardown did not run exactly once")
        }
        try await checkReleasedAfterFailure()
        try await checkReleasedAfterCancellation()
        print("PASS: overlap rejects without read, ACK or close; success/error/cancel release")
    }
}
"""
with tempfile.TemporaryDirectory(prefix="xmtp-reader-overlap-") as directory:
    path = Path(directory)
    swift = path / "Checks.swift"
    swift.write_text("import Foundation\n" + stubs + source + checks)
    binary = path / "checks"
    subprocess.run(
        ["swiftc", "-parse-as-library", str(swift), "-o", str(binary)], check=True
    )
    subprocess.run([str(binary)], check=True)
