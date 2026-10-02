import Foundation
@testable import XmtpSdk

func checkLifecycleStartupBarrier() async throws {
    let entered = TestFlag()
    let started = TestCounter()
    let returned = TestCounter()
    let suspends = TestCounter()
    let (release, signal) = AsyncStream<Void>.makeStream()
    let manager = StreamLifecycleManager(suspend: {
        suspends.increment()
        entered.set()
        var iterator = release.makeAsyncIterator()
        _ = await iterator.next()
    }, resume: {})
    // This is the state that registration selects on a background launch.
    manager.setDesired(live: false)
    defer { signal.finish() }
    let first = Task {
        started.increment()
        await manager.enableIfNeeded()
        returned.increment()
    }
    let second = Task {
        started.increment()
        await manager.enableIfNeeded()
        returned.increment()
    }
    for _ in 0 ..< 200 where !entered.value || started.value != 2 {
        try await Task.sleep(for: .milliseconds(10))
    }
    guard entered.value, started.value == 2 else {
        signal.finish()
        await first.value
        await second.value
        throw ConformanceFailure("lifecycle startup did not reach the held suspension")
    }
    try await Task.sleep(for: .milliseconds(200))
    let returnedBeforeSuspension = returned.value
    signal.finish()
    await first.value
    await second.value
    guard returnedBeforeSuspension == 0, returned.value == 2, suspends.value == 1 else {
        throw ConformanceFailure("lifecycle startup returned before initial suspension")
    }
}

func checkLifecycleFailedSuspendResume(overlap: Bool) async throws {
    let entered = TestFlag()
    let calls = CallLog()
    let (release, signal) = AsyncStream<Void>.makeStream()
    let manager = StreamLifecycleManager(suspend: {
        // Rust sets its process latch before an acknowledgement can fail.
        calls.append("suspend")
        entered.set()
        var iterator = release.makeAsyncIterator()
        _ = await iterator.next()
        throw ConformanceFailure("injected suspend acknowledgement failure")
    }, resume: { calls.append("resume") })
    let suspension = manager.setDesired(live: false)
    defer { signal.finish() }
    for _ in 0 ..< 200 where !entered.value {
        try await Task.sleep(for: .milliseconds(10))
    }
    guard entered.value else {
        signal.finish()
        await suspension?.value
        throw ConformanceFailure("lifecycle failed suspension did not start")
    }
    if overlap {
        manager.setDesired(live: true)
    }
    signal.finish()
    await suspension?.value
    if !overlap {
        await manager.setDesired(live: true)?.value
    }
    guard calls.calls == ["suspend", "resume"] else {
        throw ConformanceFailure("lifecycle foreground did not resume after a failed suspend")
    }
}
