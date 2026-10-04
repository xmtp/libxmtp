import Foundation
@testable import XmtpSdk

// Cancellation must reach a cooperative open before its manual release.
// verifies: PROC-041
func checkCooperativeReaderOpeningCancellation() async throws {
    let entered = TestFlag()
    let cancelled = TestFlag()
    let returned = TestFlag()
    let closeCount = TestCounter()
    let (release, releaseSignal) = AsyncStream<Void>.makeStream()
    let stream: SDKMessageStream = SDKReaderStream(open: {
        entered.set()
        var iterator = release.makeAsyncIterator()
        _ = await iterator.next()
        if Task.isCancelled {
            cancelled.set()
        }
        throw CancellationError()
    }, onClose: { reason in
        if case .closed = reason {
            closeCount.increment()
        }
    }, onConnectionStateChange: nil)
    defer { releaseSignal.finish() }
    let operation = Task {
        defer { returned.set() }
        let iterator = stream.makeAsyncIterator()
        return try await iterator.next()
    }
    defer { operation.cancel() }
    for _ in 0 ..< 200 where !entered.value {
        try await Task.sleep(for: .milliseconds(10))
    }
    guard entered.value else {
        operation.cancel()
        releaseSignal.finish()
        _ = try? await operation.value
        throw ConformanceFailure("cooperative reader open did not start")
    }
    operation.cancel()
    for _ in 0 ..< 200 where !returned.value {
        try await Task.sleep(for: .milliseconds(10))
    }
    let completedWithoutRelease = returned.value
    // Release only after the deadline. This also cleans up the old-source control.
    releaseSignal.finish()
    do {
        _ = try await operation.value
        throw ConformanceFailure("cancelled reader open delivered a message")
    } catch is CancellationError {}
    guard completedWithoutRelease, cancelled.value, closeCount.value == 1 else {
        throw ConformanceFailure("reader open did not receive cancellation before manual release")
    }
    withExtendedLifetime(stream) {}
}

actor ReaderLateReadyGate {
    private var released = false
    private var waiting: CheckedContinuation<Void, Never>?

    func hold() async {
        if released {
            return
        }
        await withCheckedContinuation { continuation in
            waiting = continuation
        }
    }

    func release() {
        released = true
        waiting?.resume()
        waiting = nil
    }
}

// A late open can ignore cancellation. Close must wait for its reader to end.
// verifies: PROC-041
func checkLateReaderOpeningCleanup(owner: SDKClient, group: Group) async throws {
    let (opened, openedSignal) = AsyncStream<MessageReader>.makeStream()
    let release = ReaderLateReadyGate()
    let previousOpenHook = SDKClient.readerOpenedForTest
    defer { SDKClient.readerOpenedForTest = previousOpenHook }
    SDKClient.readerOpenedForTest = { reader in
        openedSignal.yield(reader)
        await release.hold()
    }
    let lateCloseCount = TestCounter()
    let cancelledOpening = Task {
        let openingStream = try await owner.messages(
            in: group,
            onClose: { reason in
                if case .closed = reason {
                    lateCloseCount.increment()
                }
            }
        )
        let openingIterator = openingStream.makeAsyncIterator()
        return try await openingIterator.next()
    }
    defer { cancelledOpening.cancel() }
    defer { Task { await release.release() } }
    let openDeadline = Task {
        do { try await Task.sleep(for: .seconds(10)) }
        catch { return }
        openedSignal.finish()
    }
    defer { openDeadline.cancel() }
    var openedIterator = opened.makeAsyncIterator()
    guard let lateReader = await openedIterator.next() else {
        cancelledOpening.cancel()
        await release.release()
        _ = try? await cancelledOpening.value
        throw ConformanceFailure("reader did not open before cancellation")
    }
    cancelledOpening.cancel()
    try await Task.sleep(for: .milliseconds(100))
    guard lateCloseCount.value == 0 else {
        await release.release()
        _ = try? await cancelledOpening.value
        throw ConformanceFailure("close callback ran before the late reader ended")
    }
    await release.release()
    do {
        _ = try await cancelledOpening.value
        throw ConformanceFailure("cancelled reader creation delivered a message")
    } catch is CancellationError {}
    for _ in 0 ..< 1000 {
        if await lateReader.connectionState() == .closed {
            break
        }
        try await Task.sleep(for: .milliseconds(10))
    }
    guard await lateReader.connectionState() == .closed else {
        throw ConformanceFailure("late reader was not closed")
    }
    for _ in 0 ..< 100 where lateCloseCount.value == 0 {
        try await Task.sleep(for: .milliseconds(10))
    }
    guard lateCloseCount.value == 1 else {
        throw ConformanceFailure("late reader did not notify close")
    }
    guard try await lateReader.next() == nil else {
        throw ConformanceFailure("late reader was not closed")
    }
}
