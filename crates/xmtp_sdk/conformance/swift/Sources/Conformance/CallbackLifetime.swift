import Foundation
@testable import XmtpSdk

let lifetimeCycles = 20
let lifetimeCalls = 32

actor LifetimeGate {
    private var open = false
    private var waiters: [CheckedContinuation<Void, Never>] = []
    func wait() async {
        if open {
            return
        }
        await withCheckedContinuation { waiters.append($0) }
    }

    func release() {
        open = true
        let pending = waiters
        waiters.removeAll()
        for waiter in pending {
            waiter.resume()
        }
    }
}

func lifetimeWait(_ label: String, _ ready: () -> Bool) async throws {
    let deadline = ContinuousClock.now + .seconds(30)
    while !ready() {
        guard ContinuousClock.now < deadline else { throw ConformanceFailure("\(label) timed out") }
        try await Task.sleep(for: .milliseconds(1))
    }
}

func lifetimeDrained() async throws {
    try await lifetimeWait("foreign task and callback handle drain") {
        let tasks = sdkConformanceForeignCallCounts()
        return tasks.inFlight == 0 && tasks.running == 0 && sdkConformanceCallbackHandleCounts().values.allSatisfy { $0 == 0 }
    }
    let tasks = sdkConformanceForeignCallCounts()
    guard tasks.droppedEarly == 0, tasks.pollsOnCallerThread == 0 else {
        throw ConformanceFailure("foreign callback dropped early or polled on caller thread")
    }
}

func lifetimeOptions(_ backend: BackendOptions) -> ClientOptions {
    ClientOptions(backend: .options(options: backend), storage: StorageOptions(location: .inMemory), deviceSync: false)
}

private final class LifetimeHold: @unchecked Sendable {
    let entered = TestCounter()
    let completed = TestCounter()
    let ended = TestCounter()
    let action = LifetimeGate()
    let release = LifetimeGate()
    let independent: Client
    init(_ independent: Client) {
        self.independent = independent
    }

    func run() async throws {
        entered.increment()
        defer { completed.increment() }
        await action.wait()
        try await independent.end()
        ended.increment()
        await release.wait()
    }
}

private final class LifetimeSigner: Signer, @unchecked Sendable {
    let base: Signer
    let family: String
    let hold: LifetimeHold
    init(_ base: Signer, _ family: String, _ hold: LifetimeHold) {
        self.base = base; self.family = family; self.hold = hold
    }

    func identity() async throws -> PublicIdentity {
        if family == "identity" {
            try await hold.run()
        }
        return try await base.identity()
    }

    func kind() async throws -> SignerKind {
        if family == "kind" {
            try await hold.run()
        }
        return try await base.kind()
    }

    func sign(request: SigningRequest) async throws -> Signature {
        if family == "sign" {
            try await hold.run()
        }
        return try await base.sign(request: request)
    }
}

private final class LifetimePreAuthenticate: PreAuthenticate, @unchecked Sendable {
    let hold: LifetimeHold
    init(_ hold: LifetimeHold) {
        self.hold = hold
    }

    func run() async throws {
        try await hold.run()
    }
}

private func creationLifetimeCycle(_ backend: BackendOptions, _ family: String) async throws {
    let width = family == "preAuthenticate" ? 1 : lifetimeCalls
    let independent = try await Client.create(signer: generateLocalSigner(), options: lifetimeOptions(backend))
    let hold = LifetimeHold(independent)
    var calls: [Task<Client, Error>] = []
    do {
        for _ in 0 ..< width {
            let signer = LifetimeSigner(await generateLocalSigner(), family, hold)
            var options = lifetimeOptions(backend)
            if family == "preAuthenticate" {
                options.handlers = ClientHandlers(preAuthenticate: LifetimePreAuthenticate(hold))
            }
            calls.append(Task { try await Client.create(signer: signer, options: options) })
        }
        try await lifetimeWait("\(family) entry") { hold.entered.value == width }
        guard hold.completed.value == 0, sdkConformanceForeignCallCounts().running >= UInt64(width),
              sdkConformanceCallbackHandleCounts()["foreignFutures", default: 0] >= width
        else {
            throw ConformanceFailure("held callback count is wrong")
        }
        for call in calls {
            call.cancel()
        }
        let cancelled = TestCounter()
        let observers = calls.map { call in Task { _ = await call.result; cancelled.increment() } }
        try await lifetimeWait("caller cancellation") { cancelled.value == width }
        _ = observers
        for call in calls {
            switch await call.result {
            case let .success(client):
                try await client.end()
                throw ConformanceFailure("cancelled create returned a client")
            case .failure: break
            }
        }
        guard hold.completed.value == 0 else { throw ConformanceFailure("caller cancellation ended a host callback") }
        await hold.action.release()
        try await lifetimeWait("independent client end") { hold.ended.value == width }
        guard hold.completed.value == 0 else { throw ConformanceFailure("callback returned before release") }
        await hold.release.release()
        try await lifetimeWait("callback completion") { hold.completed.value == width }
        try await independent.end()
    } catch {
        print("Swift creation failure: entered=\(hold.entered.value), completed=\(hold.completed.value), tasks=\(sdkConformanceForeignCallCounts()), handles=\(sdkConformanceCallbackHandleCounts())")
        await hold.action.release()
        await hold.release.release()
        for call in calls {
            call.cancel()
        }
        try? await independent.end()
        throw error
    }
}

private func constructorLifetimeCycle(_ backend: BackendOptions, _ directory: URL, _ build: Bool, _ cancel: Bool, _ cycle: Int) async throws {
    let path = directory.appendingPathComponent("\(build)-\(cancel)-\(cycle).db3").path
    var options = lifetimeOptions(backend)
    options.storage = StorageOptions(location: .explicit(dbPath: path, attachmentsDir: "\(path).attachments"), singleConnection: true)
    let signer = await generateLocalSigner()
    let identity = try await signer.identity()
    if build {
        let seed = try await Client.create(signer: signer, options: options)
        try await seed.end()
    }
    let probe = await SdkConformanceConstructorProbe.open()
    let call = Task {
        if build {
            return try await probe.build(identity: identity, options: options, inboxId: nil)
        }
        return try await probe.create(signer: signer, options: options)
    }
    do {
        try await probe.waitForCompleted()
        let held = probe.state()
        guard held.clientCaptured, held.storeConnected, !held.clientClosed, !held.workersStopped, !held.storeOpenReported else {
            throw ConformanceFailure("constructor completion state is wrong")
        }
        if cancel {
            call.cancel()
            let cancelled = TestFlag()
            let observer = Task { _ = await call.result; cancelled.set() }
            try await lifetimeWait("constructor cancellation") { cancelled.value }
            _ = observer
            switch await call.result {
            case .success:
                throw ConformanceFailure("cancelled constructor succeeded")
            case .failure: break
            }
            try await probe.waitForCleanup()
            let cleaned = probe.state()
            guard cleaned.storeOpenReported, cleaned.clientClosed, cleaned.workersStopped, !cleaned.storeConnected else {
                throw ConformanceFailure("unadopted constructor did not close its store")
            }
        } else {
            probe.release()
            try await call.value
            guard probe.state().storeConnected, !probe.state().storeOpenReported else { throw ConformanceFailure("adopted client store is closed") }
            try await probe.endAdopted()
        }
        try await probe.cleanup()
    } catch {
        call.cancel()
        try? await probe.cleanup()
        throw error
    }
}

func checkCallbackLifetime(backend: BackendOptions) async throws {
    let selected = ProcessInfo.processInfo.environment["SDK_CALLBACK_LIFETIME_FAMILY"]
    for family in ["identity", "kind", "sign", "preAuthenticate"] {
        if let selected, selected != family {
            continue
        }
        for _ in 0 ..< lifetimeCycles {
            try await creationLifetimeCycle(backend, family)
            try await lifetimeDrained()
        }
        print("Swift callback lifetime: \(family), \(lifetimeCycles) cycles passed")
    }
    try await callbackLifetimeOperations(backend, selected)
    try await callbackReadyResult(backend, selected)
    if selected == nil || selected == "constructorFailure" {
        try await constructorFailure(backend)
    }
    if selected == nil || selected == "adoption" {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent("xmtp-swift-adoption-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        for build in [false, true] {
            for cancel in [false, true] {
                for cycle in 0 ..< lifetimeCycles {
                    try await constructorLifetimeCycle(backend, directory, build, cancel, cycle)
                    try await lifetimeDrained()
                }
                print("Swift constructor lifetime: build=\(build), cancel=\(cancel), \(lifetimeCycles) cycles passed")
            }
        }
    }
}

private final class FailingKindSigner: Signer, @unchecked Sendable {
    let base: Signer
    init(_ base: Signer) {
        self.base = base
    }

    func identity() async throws -> PublicIdentity {
        try await base.identity()
    }

    func kind() async throws -> SignerKind {
        throw SignerError.Failed
    }

    func sign(request: SigningRequest) async throws -> Signature {
        try await base.sign(request: request)
    }
}

private func constructorFailureCycle(_ backend: BackendOptions, _ path: String, _ build: Bool) async throws {
    var options = lifetimeOptions(backend)
    options.storage = StorageOptions(location: .explicit(dbPath: path, attachmentsDir: "\(path).attachments"), singleConnection: true)
    let base = await generateLocalSigner()
    let probe = await SdkConformanceConstructorProbe.open()
    let pending = Task {
        if build {
            try await probe.build(identity: base.identity(), options: options, inboxId: nil)
        } else {
            try await probe.create(signer: FailingKindSigner(base), options: options)
        }
    }
    do {
        try await probe.waitForCompleted()
        let state = probe.state()
        guard !state.storeOpenReported, !state.storeConnected else { throw ConformanceFailure("failed constructor kept store open") }
        if !build {
            guard state.clientCaptured, state.clientClosed, state.workersStopped else { throw ConformanceFailure("failed create skipped cleanup") }
        }
        probe.release()
        switch await pending.result {
        case .success: throw ConformanceFailure("invalid constructor succeeded")
        case .failure: break
        }
        try await probe.cleanup()
    } catch { try? await probe.cleanup(); throw error }
}

private func constructorFailure(_ backend: BackendOptions) async throws {
    let directory = FileManager.default.temporaryDirectory.appendingPathComponent("xmtp-swift-failure-\(UUID().uuidString)")
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: directory) }
    for build in [false, true] {
        for cycle in 0 ..< lifetimeCycles {
            try await constructorFailureCycle(backend, directory.appendingPathComponent("\(build)-\(cycle).db3").path, build)
            try await lifetimeDrained()
        }
    }
    print("Swift constructor lifetime: clean create/build failure, \(lifetimeCycles) cycles passed")
}
