import Foundation
@testable import XmtpSdk

private final class OperationHold: @unchecked Sendable {
    let entered = TestCounter()
    let completed = TestCounter()
    let reentered = TestCounter()
    let action = LifetimeGate()
    let release = LifetimeGate()
    private let operation: @Sendable () async throws -> Void
    init(_ operation: @escaping @Sendable () async throws -> Void = {}) {
        self.operation = operation
    }

    func run() async throws {
        entered.increment()
        defer { completed.increment() }
        await action.wait()
        try await operation()
        reentered.increment()
        await release.wait()
    }
}

private final class OperationListener: EventListener, @unchecked Sendable {
    let hold: OperationHold
    init(_ hold: OperationHold) {
        self.hold = hold
    }

    func onEvent(event _: ClientEvent) async throws {
        try await hold.run()
    }
}

private final class ListenerIdBox: @unchecked Sendable {
    private let lock = NSLock()
    private var value: ListenerId = 0
    func set(_ id: ListenerId) {
        lock.lock(); defer { lock.unlock() }; value = id
    }

    func get() -> ListenerId {
        lock.lock(); defer { lock.unlock() }; return value
    }
}

private func eventLifetimeCycle(_ backend: BackendOptions, _ ownEnd: Bool) async throws {
    let client = try await Client.create(signer: generateLocalSigner(), options: lifetimeOptions(backend))
    let id = ListenerIdBox()
    let hold = OperationHold {
        if ownEnd {
            try await client.end()
        } else {
            await client.stopListener(id: id.get())
        }
    }
    do {
        try id.set(await client.startListener(
            filter: EventFilter(kinds: [.hmacKeysUpdated], groupIds: nil, contentTypes: nil, referencesOwnMessages: false),
            listener: OperationListener(hold)
        ))
        _ = try client.sdkConformanceEmitHmacEvents(count: 1)
        try await lifetimeWait("event callback entry") { hold.entered.value == 1 }
        _ = try client.sdkConformanceEmitHmacEvents(count: 1030)
        let queued = client.sdkConformanceListenerCounts(id: id.get())
        guard queued.registered, queued.queued == 1023, queued.inFlight == 1, queued.discarded == 7
        else { throw ConformanceFailure("held event queue counts are wrong") }
        guard hold.entered.value == 1, hold.completed.value == 0,
              sdkConformanceForeignCallCounts().running >= 1 else { throw ConformanceFailure("serial event callback count is wrong") }
        await hold.action.release()
        try await lifetimeWait("event callback reentry") { hold.reentered.value == 1 }
        guard hold.completed.value == 0 else { throw ConformanceFailure("stop/end ended active callback") }
        guard !client.sdkConformanceListenerCounts(id: id.get()).registered
        else { throw ConformanceFailure("listener remained registered after stop/end") }
        await hold.release.release()
        try await lifetimeWait("event callback completion") { hold.completed.value == 1 }
        await Task.yield()
        guard hold.entered.value == 1 else { throw ConformanceFailure("queued callback handed off after stop/end") }
        try await client.end()
    } catch {
        await hold.action.release(); await hold.release.release()
        await client.stopListener(id: id.get()); try? await client.end()
        throw error
    }
}

private final class OperationSigner: Signer, @unchecked Sendable {
    let base: Signer
    let hold: OperationHold
    init(_ base: Signer, _ hold: OperationHold) {
        self.base = base; self.hold = hold
    }

    func identity() async throws -> PublicIdentity {
        try await base.identity()
    }

    func kind() async throws -> SignerKind {
        try await base.kind()
    }

    func sign(request: SigningRequest) async throws -> Signature {
        try await hold.run()
        return try await base.sign(request: request)
    }
}

private func signatureLifetimeCycle(_ backend: BackendOptions) async throws {
    let hold = OperationHold()
    var clients: [Client] = []
    var requests: [SignatureRequest] = []
    var calls: [Task<Void, Error>] = []
    do {
        for _ in 0 ..< lifetimeCalls {
            let base = await generateLocalSigner()
            var options = lifetimeOptions(backend)
            options.registration = RegistrationOptions(auto: false)
            let client = try await Client.create(signer: base, options: options)
            clients.append(client)
            guard let request = try await client.unsafeCreateInboxSignatureRequest() else { throw ConformanceFailure("missing signature request") }
            requests.append(request)
            calls.append(Task { try await request.sign(signer: OperationSigner(base, hold)) })
        }
        try await lifetimeWait("signature callback entry") { hold.entered.value == lifetimeCalls }
        guard sdkConformanceForeignCallCounts().running >= UInt64(lifetimeCalls) else { throw ConformanceFailure("held signature tasks missing") }
        for client in clients {
            try await client.end()
        }
        guard hold.completed.value == 0 else { throw ConformanceFailure("owner end ended active signature callback") }
        await hold.action.release(); await hold.release.release()
        for call in calls {
            try await call.value
        }
        for (client, request) in zip(clients, requests) {
            do {
                try await client.unsafeApplySignatureRequest(request: request)
                throw ConformanceFailure("signature applied after owner end")
            } catch XmtpError.ClientClosed {}
        }
    } catch {
        await hold.action.release(); await hold.release.release()
        for call in calls {
            call.cancel()
        }
        for client in clients {
            try? await client.end()
        }
        throw error
    }
}

private final class OperationCredential: CredentialSource, @unchecked Sendable {
    let armed = TestFlag()
    let hold: OperationHold
    init(_ hold: OperationHold) {
        self.hold = hold
    }

    func credential() async throws -> Credential {
        if armed.value {
            try await hold.run()
        }
        return Credential(name: nil, value: "Bearer callback-lifetime", expiresAtSeconds: 9_000_000_000_000_000)
    }
}

/// The credential callback can end an independent client. It must return before
/// its own client ends because that end waits for the callback's foreground call.
private func credentialLifetimeCycle(_ backend: BackendOptions) async throws {
    let independent = try await Client.create(signer: generateLocalSigner(), options: lifetimeOptions(backend))
    let hold = OperationHold { try await independent.end() }
    var clients: [Client] = []
    var groups: [Group] = []
    var calls: [Task<Void, Error>] = []
    do {
        for _ in 0 ..< lifetimeCalls {
            let source = OperationCredential(hold)
            var options = lifetimeOptions(backend)
            var authenticated = backend
            authenticated.credentials = source
            options.backend = .options(options: authenticated)
            let client = try await Client.create(signer: generateLocalSigner(), options: options)
            clients.append(client)
            let group = try await client.conversations().createGroup(members: [InboxId]())
            groups.append(group)
            try await client.setCredential(credential: Credential(name: nil, value: "Bearer expired", expiresAtSeconds: 0))
            source.armed.set()
            calls.append(Task { try await group.sync() })
        }
        try await lifetimeWait("credential callback entry") { hold.entered.value == lifetimeCalls }
        guard sdkConformanceForeignCallCounts().running >= UInt64(lifetimeCalls) else { throw ConformanceFailure("held credential tasks missing") }
        for call in calls {
            call.cancel()
        }
        let cancelled = TestCounter()
        let observers = calls.map { call in Task { _ = await call.result; cancelled.increment() } }
        try await lifetimeWait("credential caller cancellation") { cancelled.value == lifetimeCalls }
        _ = observers
        for call in calls {
            switch await call.result {
            case .success: throw ConformanceFailure("cancelled group sync succeeded")
            case .failure: break
            }
        }
        guard hold.completed.value == 0 else { throw ConformanceFailure("caller cancellation ended credential callback") }
        await hold.action.release()
        try await lifetimeWait("credential independent end reentry") { hold.reentered.value == lifetimeCalls }
        let ended = TestCounter()
        let ends = clients.map { client in Task { try await client.end(); ended.increment() } }
        await Task.yield()
        guard hold.completed.value == 0, ended.value == 0 else { throw ConformanceFailure("owner end skipped held credential callbacks") }
        await hold.release.release()
        for end in ends {
            try await end.value
        }
        for group in groups {
            do { try await group.sync(); throw ConformanceFailure("sync succeeded after owner end") }
            catch XmtpError.ClientClosed {}
        }
    } catch {
        await hold.action.release(); await hold.release.release()
        for call in calls {
            call.cancel()
        }
        for client in clients {
            try? await client.end()
        }
        try? await independent.end()
        throw error
    }
}

func callbackLifetimeOperations(_ backend: BackendOptions, _ selected: String?) async throws {
    for family in ["eventStop", "eventEnd", "signatureRequest", "credential"] {
        if let selected, selected != family {
            continue
        }
        for _ in 0 ..< lifetimeCycles {
            switch family {
            case "signatureRequest": try await signatureLifetimeCycle(backend)
            case "credential": try await credentialLifetimeCycle(backend)
            default: try await eventLifetimeCycle(backend, family == "eventEnd")
            }
            try await lifetimeDrained()
        }
        print("Swift callback lifetime: \(family), \(lifetimeCycles) cycles passed")
    }
}
