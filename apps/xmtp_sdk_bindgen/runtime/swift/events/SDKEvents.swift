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
    private var active: [ListenerID: ListenerStartGate] = [:]
    private var pending: [ListenerStartGate] = []

    func addPending(_ gate: ListenerStartGate) {
        lock.lock()
        pending.append(gate)
        lock.unlock()
    }

    func registered(_ id: ListenerID, gate: ListenerStartGate) {
        lock.lock()
        pending.removeAll { $0 === gate }
        active[id] = gate
        lock.unlock()
    }

    func discard(_ gate: ListenerStartGate) {
        lock.lock()
        pending.removeAll { $0 === gate }
        lock.unlock()
    }

    func stop(_ id: ListenerID) {
        lock.lock()
        active.removeValue(forKey: id)?.stop()
        lock.unlock()
    }

    func stopAll() {
        lock.lock()
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
    ) async throws -> ListenerID {
        let gate = ListenerStartGate()
        listenerGates.addPending(gate)
        defer { listenerGates.discard(gate) }
        let id = try await raw.startListener(filter: filter, listener: ClosureEventListener(onEvent, gate: gate))
        listenerGates.registered(id, gate: gate)
        return id
    }

    func stopListener(_ id: ListenerID) async {
        listenerGates.stop(id)
        await raw.stopListener(id: id)
    }
}

/// Each iterator request reads one event from the Rust subscription.
public struct SDKEventStream: AsyncSequence {
    public typealias Element = ClientEvent
    private let reader: EventReader
    private let owner: SDKClient

    fileprivate init(reader: EventReader, owner: SDKClient) {
        self.reader = reader
        self.owner = owner
    }

    public func makeAsyncIterator() -> Iterator {
        Iterator(reader: reader, owner: owner)
    }

    public final class Iterator: AsyncIteratorProtocol {
        private let reader: EventReader
        private let owner: SDKClient
        private var closed = false

        fileprivate init(reader: EventReader, owner: SDKClient) {
            self.reader = reader
            self.owner = owner
        }

        public func next() async throws -> ClientEvent? {
            if closed {
                return nil
            }
            return try await withTaskCancellationHandler {
                _ = owner.raw
                let value = try await reader.next()
                if value == nil {
                    closed = true
                    try await reader.end()
                }
                return value
            } onCancel: {
                Task { try? await reader.end() }
            }
        }

        deinit {
            let reader = reader
            Task { try? await reader.end() }
        }
    }
}
