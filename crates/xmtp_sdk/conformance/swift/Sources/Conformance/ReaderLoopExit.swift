import Foundation
@testable import XmtpSdk

// The stored sequence stays alive after the app throws. Its loop iterator
// must release the reader. The app error must not acknowledge the item.
// verifies: PROC-052, PROC-031, PROC-041
func checkReaderAppError(owner: SDKClient, group: Group, messageId: MessageId) async throws {
    // Keep the raw handle alive so native destruction cannot hide a missing
    // adapter end call.
    let (opened, openedSignal) = AsyncStream<MessageReader>.makeStream()
    let previousOpenHook = SDKClient.readerOpenedForTest
    SDKClient.readerOpenedForTest = { reader in
        openedSignal.yield(reader)
        openedSignal.finish()
    }
    defer { SDKClient.readerOpenedForTest = previousOpenHook }
    let (closed, closeSignal) = AsyncStream<SDKStreamCloseReason>.makeStream()
    let closeCount = TestCounter()
    let stream = try await owner.messages(in: group, onClose: { reason in
        closeCount.increment()
        closeSignal.yield(reason)
        closeSignal.finish()
    })
    do {
        for try await message in stream {
            guard message.id == messageId else {
                throw ConformanceFailure("app-error loop received the wrong message")
            }
            throw ConformanceFailure("app stopped the loop")
        }
        throw ConformanceFailure("app-error loop ended without an item")
    } catch let error as ConformanceFailure where error.errorDescription == "app stopped the loop" {}

    // Deinit starts asynchronous cleanup. Wait for its completion signal;
    // returning from the loop itself does not prove teardown has finished.
    let deadline = Task {
        do { try await Task.sleep(for: .seconds(10)) }
        catch { return }
        closeSignal.finish()
    }
    defer { deadline.cancel() }
    var closeIterator = closed.makeAsyncIterator()
    guard let reason = await closeIterator.next(), case .closed = reason else {
        throw ConformanceFailure("app-error loop did not close its reader")
    }
    guard closeCount.value == 1 else {
        throw ConformanceFailure("app-error loop notified close more than once")
    }
    var openedIterator = opened.makeAsyncIterator()
    guard let rawReader = await openedIterator.next(), await rawReader.connectionState() == .closed else {
        throw ConformanceFailure("app-error loop did not end the retained raw reader")
    }
    let replay = try await group.messageReader()
    let replayDeadline = Task {
        do { try await Task.sleep(for: .seconds(10)) }
        catch { return }
        try? await replay.end()
    }
    let repeated = try await replay.next()
    replayDeadline.cancel()
    try await replay.end()
    withExtendedLifetime((stream, rawReader)) {}
    guard repeated?.id == messageId else {
        throw ConformanceFailure("app-error loop acknowledged the held item")
    }
}
