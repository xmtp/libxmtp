import Foundation
@testable import XmtpSdk

private func readyResultCycle(_ backend: BackendOptions, _ build: Bool) async throws {
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
    await sdkConformanceSwiftReadyGate.reset()
    let call = Task {
        if build {
            return try await probe.buildReady(identity: identity, options: options, inboxId: nil)
        }
        return try await probe.createReady(signer: signer, options: options)
    }
    do {
        let deadline = ContinuousClock.now + .seconds(30)
        while !(await sdkConformanceSwiftReadyGate.didEnter()) {
            guard ContinuousClock.now < deadline else { throw ConformanceFailure("constructor READY gate timed out") }
            try await Task.sleep(for: .milliseconds(1))
        }
        guard probe.readyClientAlive() else { throw ConformanceFailure("READY result has no native client owner") }
        call.cancel()
        await sdkConformanceSwiftReadyGate.release()
        switch await call.result {
        case .success:
            throw ConformanceFailure("cancelled READY constructor succeeded")
        case let .failure(error):
            guard error is CancellationError else { throw error }
        }
        try await lifetimeWait("READY client owner release") { !probe.readyClientAlive() }
        try await probe.cleanup()
    } catch {
        call.cancel()
        await sdkConformanceSwiftReadyGate.release()
        _ = await call.result
        try? await probe.cleanup()
        throw error
    }
}

func callbackReadyResult(_ backend: BackendOptions, _ selected: String?) async throws {
    guard selected == nil || selected == "readyResult" else { return }
    for build in [false, true] {
        for _ in 0 ..< lifetimeCycles {
            try await readyResultCycle(backend, build)
            try await lifetimeDrained()
        }
        print("Swift READY result: build=\(build), \(lifetimeCycles) cycles passed")
    }
}
