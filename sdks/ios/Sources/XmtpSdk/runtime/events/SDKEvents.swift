import Foundation

final class ListenerStartGate: @unchecked Sendable {
    private let lock = NSLock()
    private var stopped = false

    func begin() -> Bool {
        lock.lock()
        defer { lock.unlock() }
        return !stopped
    }

    func stop() {
        lock.lock()
        stopped = true
        lock.unlock()
    }
}

final class ListenerGates: @unchecked Sendable {
    private let lock = NSLock()
    private var active: [ListenerId: ListenerStartGate] = [:]
    private var pending: [ListenerStartGate] = []
    private var closed = false

    func addPending(_ gate: ListenerStartGate) {
        lock.lock()
        if closed {
            gate.stop()
        } else {
            pending.append(gate)
        }
        lock.unlock()
    }

    func registered(_ id: ListenerId, gate: ListenerStartGate) {
        lock.lock()
        pending.removeAll { $0 === gate }
        if closed {
            gate.stop()
        } else {
            active[id] = gate
        }
        lock.unlock()
    }

    func discard(_ gate: ListenerStartGate) {
        lock.lock()
        pending.removeAll { $0 === gate }
        lock.unlock()
    }

    func stop(_ id: ListenerId) {
        lock.lock()
        active.removeValue(forKey: id)?.stop()
        lock.unlock()
    }

    func stopAll() {
        lock.lock()
        closed = true
        active.values.forEach { $0.stop() }
        pending.forEach { $0.stop() }
        active.removeAll()
        pending.removeAll()
        lock.unlock()
    }
}

private final class ClosureEventListener: EventListener, @unchecked Sendable {
    let callback: @Sendable (ClientEvent) async throws -> Void
    let gate: ListenerStartGate

    init(_ callback: @escaping @Sendable (ClientEvent) async throws -> Void, gate: ListenerStartGate) {
        self.callback = callback
        self.gate = gate
    }

    func onEvent(event: ClientEvent) async throws {
        guard gate.begin() else { return }
        do {
            try await callback(event)
        } catch {
            throw ListenerError.Failed
        }
    }
}

public extension SDKClient {
    func events(_ filter: EventFilter) async throws -> SDKEventStream {
        try SDKEventStream(reader: await raw.events(filter: filter), owner: self)
    }

    func startListener(
        _ filter: EventFilter,
        onEvent: @escaping @Sendable (ClientEvent) async throws -> Void
    ) async throws -> ListenerId {
        let gate = ListenerStartGate()
        listenerGates.addPending(gate)
        defer { listenerGates.discard(gate) }
        let id = try await raw.startListener(filter: filter, listener: ClosureEventListener(onEvent, gate: gate))
        listenerGates.registered(id, gate: gate)
        return id
    }

    func stopListener(_ id: ListenerId) async {
        listenerGates.stop(id)
        await raw.stopListener(id: id)
    }
}

private final class EventIteratorClaim {
    private let lock = NSLock()
    private var claimed = false

    func acquire() -> Bool {
        lock.lock()
        defer { lock.unlock() }
        guard !claimed else { return false }
        claimed = true
        return true
    }
}

/// The first iterator owns this event subscription.
public struct SDKEventStream: AsyncSequence {
    public typealias Element = ClientEvent
    private let reader: EventReader
    private let owner: SDKClient
    private let claim = EventIteratorClaim()

    fileprivate init(reader: EventReader, owner: SDKClient) {
        self.reader = reader
        self.owner = owner
    }

    public func makeAsyncIterator() -> Iterator {
        Iterator(reader: reader, owner: owner, ownsReader: claim.acquire())
    }

    public final class Iterator: AsyncIteratorProtocol {
        private let reader: EventReader
        private let owner: SDKClient
        private let ownsReader: Bool
        private var closed = false
        private let readLock = NSLock()
        private var readInFlight = false

        fileprivate init(reader: EventReader, owner: SDKClient, ownsReader: Bool) {
            self.reader = reader
            self.owner = owner
            self.ownsReader = ownsReader
        }

        // implements: EVENT-015
        private func beginRead() throws {
            readLock.lock()
            defer { readLock.unlock() }
            guard !readInFlight else {
                throw XmtpError.ConsumerOwned(ErrorDetails(
                    code: "ConsumerOwned", category: .stream, retryable: false,
                    message: "event iterator read is active"
                ))
            }
            readInFlight = true
        }

        private func finishRead() {
            readLock.lock()
            readInFlight = false
            readLock.unlock()
        }

        public func next() async throws -> ClientEvent? {
            guard ownsReader else {
                throw XmtpError.ConsumerOwned(ErrorDetails(
                    code: "ConsumerOwned", category: .stream, retryable: false,
                    message: "event subscription already has an iterator"
                ))
            }
            try beginRead()
            defer { finishRead() }
            if closed {
                return nil
            }
            _ = owner.raw
            let value = try await reader.next()
            if value == nil {
                closed = true
                try await reader.end()
            }
            return value
        }

        deinit {
            guard ownsReader else { return }
            let reader = reader
            Task { try? await reader.end() }
        }
    }
}
