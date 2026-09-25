import Foundation

private final class ClosureEventListener: EventListener, @unchecked Sendable {
    let callback: @Sendable (ClientEvent) async throws -> Void

    init(_ callback: @escaping @Sendable (ClientEvent) async throws -> Void) {
        self.callback = callback
    }

    func onEvent(event: ClientEvent) async throws {
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
        try await raw.startListener(filter: filter, listener: ClosureEventListener(onEvent))
    }

    func stopListener(_ id: ListenerID) async {
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
