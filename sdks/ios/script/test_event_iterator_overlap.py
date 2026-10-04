#!/usr/bin/env python3
"""Check read order against the maintained Swift event iterator."""
# verifies: EVENT-015

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
source = args.source.read_text().split("private final class EventIteratorClaim", 1)[1]
source = "private final class EventIteratorClaim" + source
stubs = """
import Foundation
public enum ClientEvent: Equatable { case marker(Int) }
public final class SDKClient: @unchecked Sendable { let raw: Void = () }
public enum ErrorCategory { case stream }
public struct ErrorDetails {
    let code: String
    let category: ErrorCategory
    let retryable: Bool
    let message: String?
}
public enum XmtpError: Error { case ConsumerOwned(ErrorDetails) }
public final class EventReader: @unchecked Sendable {
    let read: @Sendable () async throws -> ClientEvent?
    let close: @Sendable () async -> Void
    init(read: @escaping @Sendable () async throws -> ClientEvent?,
         close: @escaping @Sendable () async -> Void) {
        self.read = read
        self.close = close
    }
    func next() async throws -> ClientEvent? { try await read() }
    func end() async throws { await close() }
}
"""
checks = """
enum ReadFailure: Error { case failed }
actor HeldReader {
    private var calls = 0
    private var ends = 0
    private var ended = false
    private var entered = false
    private var enteredWaiter: CheckedContinuation<Void, Never>?
    private var release: CheckedContinuation<Void, Never>?
    private let failFirst: Bool
    init(failFirst: Bool = false) { self.failFirst = failFirst }
    func next() async throws -> ClientEvent? {
        calls += 1
        if calls == 1 {
            entered = true
            enteredWaiter?.resume()
            enteredWaiter = nil
            await withCheckedContinuation { release = $0 }
            if ended { return nil }
            try Task.checkCancellation()
            if failFirst { throw ReadFailure.failed }
            return .marker(1)
        }
        return calls == 2 ? .marker(2) : nil
    }
    func waitForRead() async {
        if entered { return }
        await withCheckedContinuation { enteredWaiter = $0 }
    }
    func releaseRead() { release?.resume(); release = nil }
    func end() { ended = true; ends += 1; release?.resume(); release = nil }
    func counts() -> (Int, Int) { (calls, ends) }
}
func makeStream(_ held: HeldReader) -> SDKEventStream {
    SDKEventStream(reader: EventReader(
        read: { try await held.next() }, close: { await held.end() }
    ), owner: SDKClient())
}
func checkOverlap() async throws {
    let held = HeldReader()
    let iterator = makeStream(held).makeAsyncIterator()
    let first = Task { try await iterator.next() }
    await held.waitForRead()
    var rejected = false
    var secondValue: ClientEvent?
    do { secondValue = try await iterator.next() }
    catch let XmtpError.ConsumerOwned(details) {
        rejected = details.code == "ConsumerOwned" && !details.retryable
    }
    let beforeRelease = await held.counts()
    await held.releaseRead()
    let firstValue = try await first.value
    guard rejected else {
        fatalError("overlapping event next returned \(String(describing: secondValue)) before first \(String(describing: firstValue))")
    }
    guard beforeRelease.0 == 1, beforeRelease.1 == 0 else {
        fatalError("rejected event read advanced or ended the active reader")
    }
    guard firstValue == .marker(1), try await iterator.next() == .marker(2) else {
        fatalError("event read ownership was not released after success")
    }
    guard try await iterator.next() == nil else { fatalError("event stream did not end") }
    let afterEnd = await held.counts()
    guard afterEnd.1 == 1 else { fatalError("event reader did not end once") }
}
func checkFailureAndCancellation(_ cancel: Bool) async throws {
    let held = HeldReader(failFirst: !cancel)
    let iterator = makeStream(held).makeAsyncIterator()
    let first = Task { try await iterator.next() }
    await held.waitForRead()
    if cancel { first.cancel() }
    await held.releaseRead()
    do { _ = try await first.value; fatalError("first read did not fail") }
    catch is CancellationError { guard cancel else { throw CancellationError() } }
    catch ReadFailure.failed { guard !cancel else { throw ReadFailure.failed } }
    guard try await iterator.next() == .marker(2) else {
        fatalError("event read ownership was not released after error or cancellation")
    }
}

func checkEndRace() async throws {
    let held = HeldReader()
    let iterator = makeStream(held).makeAsyncIterator()
    let first = Task { try await iterator.next() }
    await held.waitForRead()
    do { _ = try await iterator.next(); fatalError("read during end race did not reject") }
    catch XmtpError.ConsumerOwned {}
    await held.end()
    guard try await first.value == nil, try await iterator.next() == nil else {
        fatalError("ended event iterator returned an event or kept read ownership")
    }
    let counts = await held.counts()
    guard counts.0 == 1 else { fatalError("ended event iterator read again") }
}

@main struct Checks {
    static func main() async throws {
        try await checkOverlap()
        try await checkFailureAndCancellation(false)
        try await checkFailureAndCancellation(true)
        try await checkEndRace()
        print("PASS: event overlap rejects without read or close; success/error/cancel release")
    }
}
"""
with tempfile.TemporaryDirectory(prefix="xmtp-event-overlap-") as directory:
    path = Path(directory)
    swift = path / "Checks.swift"
    swift.write_text(stubs + source + checks)
    binary = path / "checks"
    subprocess.run(
        ["swiftc", "-parse-as-library", str(swift), "-o", str(binary)], check=True
    )
    subprocess.run([str(binary)], check=True)
