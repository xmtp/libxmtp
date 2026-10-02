import Foundation
@testable import XmtpSdk

private func readyResultCycle(_ backend: BackendOptions, _ build: Bool, _ afterLift: Bool) async throws {
    let directory = FileManager.default.temporaryDirectory.appendingPathComponent("xmtp-swift-ready-\(UUID().uuidString)")
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: directory) }
    let path = directory.appendingPathComponent("client.db3").path
    var options = lifetimeOptions(backend)
    options.storage = StorageOptions(location: .explicit(dbPath: path, attachmentsDir: "\(path).attachments"), singleConnection: true)
    let signer = await generateLocalSigner()
    let identity = try await signer.identity()
    if build {
        let seed = try await Client.create(signer: signer, options: options)
        try await seed.end()
    }
    let probe = await SdkConformanceConstructorProbe.open()
    await sdkConformanceSwiftReadyGate.release()
    await sdkConformanceSwiftLiftedGate.release()
    let gate = afterLift ? sdkConformanceSwiftLiftedGate : sdkConformanceSwiftReadyGate
    await gate.reset()
    sdkConformanceSwiftConstructorCalls.reset()
    let call = Task {
        if build {
            return try await probe.buildReady(identity: identity, options: options, inboxId: nil)
        }
        return try await probe.createReady(signer: signer, options: options)
    }
    do {
        let deadline = ContinuousClock.now + .seconds(30)
        while !(await gate.didEnter()) {
            guard ContinuousClock.now < deadline else { throw ConformanceFailure("constructor READY gate timed out") }
            try await Task.sleep(for: .milliseconds(1))
        }
        guard probe.readyClientAlive() else { throw ConformanceFailure("READY result has no native client owner") }
        call.cancel()
        await gate.release()
        switch try await callerResult(call, "cancelled constructor result") {
        case .success:
            throw ConformanceFailure("cancelled READY constructor succeeded")
        case let .failure(error):
            guard error is CancellationError else { throw error }
        }
        try await lifetimeWait("READY client owner release") { !probe.readyClientAlive() }
        let state = probe.state()
        guard state.clientClosed, state.workersStopped, !state.storeConnected else {
            throw ConformanceFailure("discarded lifted Client skipped native cleanup")
        }
        let counts = sdkConformanceSwiftConstructorCalls.snapshot()
        if afterLift {
            guard counts["complete"] == 1, counts["free"] == 1, counts["cancel", default: 0] == 0 else {
                throw ConformanceFailure("post-lift constructor native calls \(counts)")
            }
        }
        print("Swift constructor: afterLift=\(afterLift), build=\(build), native=\(counts), closed=\(state.clientClosed), workersStopped=\(state.workersStopped), storeConnected=\(state.storeConnected)")
        try await probe.cleanup()
    } catch {
        call.cancel()
        await gate.release()
        _ = try? await callerResult(call, "failed constructor result")
        try? await probe.cleanup()
        throw error
    }
}

func callbackReadyResult(_ backend: BackendOptions, _ selected: String?) async throws {
    guard selected == nil || selected == "readyResult" else { return }
    for afterLift in [false, true] {
        for build in [false, true] {
            for _ in 0 ..< lifetimeCycles {
                try await readyResultCycle(backend, build, afterLift)
                try await lifetimeDrained()
            }
            print("Swift READY result: afterLift=\(afterLift), build=\(build), \(lifetimeCycles) cycles passed")
        }
    }
}
