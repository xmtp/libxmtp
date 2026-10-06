import Foundation
@testable import XmtpSdk

// These proofs use seams that only the conformance copy of the runtime has:
// `inject_reader_gate.py` exposes `StreamHandle`, the `SDKReaderStream`
// initializer and the reader-open hooks, and `inject_event_start_hook.py`
// adds `EventStartHookForTest`.

/// When iteration ends, the reader is already released: a replacement reader
/// on the same group opens at once. A slow end makes a detached teardown lose
/// this race every time.
func checkReaderReleasedBeforeIterationEnds(owner: SDKClient) async throws {
    let scope = try await owner.conversations().createGroup(members: [InboxId](), options: nil)
    func slowEndStream(
        next: @escaping @Sendable () async throws -> Message?
    ) -> SDKMessageStream {
        SDKReaderStream(open: {
            let reader = try await scope.messageReader()
            return StreamHandle(
                owner: owner,
                next: next,
                end: {
                    try? await Task.sleep(for: .milliseconds(200))
                    try? await reader.end()
                },
                connectionState: { await reader.connectionState() },
                connectionStateChanged: { try await reader.connectionStateChanged(previous: $0) }
            )
        }, onClose: nil, onConnectionStateChange: nil)
    }
    func reopenAfter(_ path: String) async throws {
        do {
            let replacement = try await scope.messageReader()
            try await replacement.end()
        } catch XmtpError.ConsumerOwned {
            throw ConformanceFailure("\(path) ended iteration before the reader was released")
        }
    }
    guard try await slowEndStream(next: { nil }).makeAsyncIterator().next() == nil else {
        throw ConformanceFailure("ended stream delivered a message")
    }
    try await reopenAfter("end of stream")
    do {
        _ = try await slowEndStream(next: { throw ConformanceFailure("read failed") })
            .makeAsyncIterator().next()
        throw ConformanceFailure("failed read delivered a message")
    } catch let failure as ConformanceFailure where failure.errorDescription == "read failed" {}
    try await reopenAfter("read failure")
    let cancelledRead = Task {
        try await slowEndStream(next: {
            try await Task.sleep(for: .seconds(10))
            return nil
        }).makeAsyncIterator().next()
    }
    try await Task.sleep(for: .milliseconds(100))
    cancelledRead.cancel()
    _ = try? await cancelledRead.value
    try await reopenAfter("cancellation")
    print("Swift reader released before iteration ends passed")
}

/// A conversation stream delivers a group created after its reader opens.
func checkConversationReaderOpensBeforeDelivery(owner: SDKClient) async throws {
    let conversationOpen = TestFlag()
    SDKClient.conversationReaderOpeningForTest = {
        try? await Task.sleep(for: .milliseconds(300))
    }
    SDKClient.conversationReaderOpenedForTest = { _ in
        conversationOpen.set()
    }
    defer {
        SDKClient.conversationReaderOpeningForTest = nil
        SDKClient.conversationReaderOpenedForTest = nil
    }
    let conversationStream = try await owner.conversationStream()
    let conversationIterator = conversationStream.makeAsyncIterator()
    let conversationPending = Task { try await conversationIterator.next() }
    let conversationDeadline = Task {
        do { try await Task.sleep(for: .seconds(10)) }
        catch { return }
        conversationPending.cancel()
    }
    defer { conversationDeadline.cancel() }
    for _ in 0 ..< 1000 {
        if conversationOpen.value {
            break
        }
        try await Task.sleep(for: .milliseconds(10))
    }
    guard conversationOpen.value else {
        conversationPending.cancel()
        throw ConformanceFailure("conversation reader was not open before group creation")
    }
    _ = try await owner.conversations().createGroup(members: [InboxId](), options: nil)
    do {
        guard try await conversationPending.value != nil else {
            throw ConformanceFailure("conversation stream missed a stored group")
        }
    } catch is CancellationError {
        throw ConformanceFailure("conversation stream did not deliver a group before the deadline")
    }
    print("Swift conversation reader opens before delivery passed")
}

/// The state monitor stops reading after Closed. A reader opened on a
/// connected connection reports Connected first.
// verifies: PROC-044
func checkConnectionStateMonitor(owner: SDKClient) async throws {
    let monitorCalls = TestCounter()
    let (monitorClosed, monitorClosedSignal) = AsyncStream<Void>.makeStream()
    let fakeHandle = StreamHandle<Int>(
        owner: owner,
        next: {
            try await Task.sleep(for: .seconds(10))
            return nil
        },
        end: {},
        connectionState: { .connecting },
        connectionStateChanged: { _ in
            monitorCalls.increment()
            return .closed
        }
    )
    let fakeStream = SDKReaderStream<Int>(
        open: { fakeHandle },
        onClose: nil,
        onConnectionStateChange: { _, current in
            if current == .closed {
                monitorClosedSignal.yield(())
            }
        }
    )
    let fakeRead = Task {
        let iterator = fakeStream.makeAsyncIterator()
        return try await iterator.next()
    }
    let monitorDeadline = Task {
        try? await Task.sleep(for: .seconds(5))
        monitorClosedSignal.finish()
    }
    var monitorClosedIterator = monitorClosed.makeAsyncIterator()
    guard await monitorClosedIterator.next() != nil else {
        fakeRead.cancel()
        throw ConformanceFailure("state monitor did not report Closed")
    }
    monitorDeadline.cancel()
    let callsAtClosed = monitorCalls.value
    try await Task.sleep(for: .milliseconds(100))
    fakeRead.cancel()
    _ = try? await fakeRead.value
    guard monitorCalls.value == callsAtClosed else {
        throw ConformanceFailure("state monitor kept reading after Closed")
    }

    let (connectedStates, connectedStateSignal) = AsyncStream<ConnectionState>.makeStream()
    let connectedHandle = StreamHandle<Int>(
        owner: owner,
        next: {
            try await Task.sleep(for: .seconds(10))
            return nil
        },
        end: {},
        connectionState: { .connected },
        connectionStateChanged: { _ in
            try await Task.sleep(for: .seconds(10))
            return .closed
        }
    )
    let connectedStream = SDKReaderStream<Int>(
        open: { connectedHandle },
        onClose: nil,
        onConnectionStateChange: { _, current in
            connectedStateSignal.yield(current)
        }
    )
    let connectedRead = Task {
        let iterator = connectedStream.makeAsyncIterator()
        return try await iterator.next()
    }
    let connectedDeadline = Task {
        try? await Task.sleep(for: .seconds(5))
        connectedStateSignal.finish()
    }
    var connectedStateIterator = connectedStates.makeAsyncIterator()
    let firstConnectedState = await connectedStateIterator.next()
    connectedDeadline.cancel()
    connectedRead.cancel()
    _ = try? await connectedRead.value
    guard firstConnectedState == .connected else {
        throw ConformanceFailure(
            "connected reader first reported \(String(describing: firstConnectedState))"
        )
    }
    print("Swift connection state monitor passed")
}

/// A listener callback held at its start does not run after stop returns.
// verifies: EVENT-053
func checkDelayedListenerStop(owner: SDKClient) async throws {
    let eventFilter = EventFilter(
        kinds: [.conversationJoined], groupIds: nil,
        contentTypes: nil, referencesOwnMessages: false
    )
    let startPause = EventStartPause()
    await EventStartHookForTest.shared.set {
        await startPause.hold()
    }
    let lateCalls = EventSignal()
    let delayedId = try await owner.startListener(eventFilter) { _ in
        await lateCalls.mark()
    }
    _ = try await owner.conversations().createGroup(members: [InboxId](), options: nil)
    try await startPause.waitUntilEntered()
    await owner.stopListener(delayedId)
    await startPause.release()
    await EventStartHookForTest.shared.set(nil)
    try await Task.sleep(nanoseconds: 100_000_000)
    guard !(await lateCalls.hasRun()) else {
        throw ConformanceFailure("callback started after stop returned")
    }
    print("Swift delayed listener stop passed")
}
