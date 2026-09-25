import Foundation

public typealias SDKMessageStream = AsyncThrowingStream<Message, Error>
public typealias SDKConversationStream = AsyncThrowingStream<Conversation, Error>

public enum SDKStreamCloseReason: @unchecked Sendable {
    case closed
    case failed(Error)
}

private extension AsyncThrowingStream where Failure == Error {
    init(
        unfolding: @escaping @Sendable () async throws -> Element?,
        onCancel: @escaping @Sendable () -> Void
    ) {
        self.init(unfolding: {
            try await withTaskCancellationHandler(operation: {
                try await unfolding()
            }, onCancel: onCancel)
        })
    }
}

private final class StreamCompletion: @unchecked Sendable {
    private let lock = NSLock()
    private var didClose = false
    private let callback: (@Sendable (SDKStreamCloseReason) -> Void)?

    init(_ callback: (@Sendable (SDKStreamCloseReason) -> Void)?) {
        self.callback = callback
    }

    func close(_ reason: SDKStreamCloseReason) {
        lock.lock()
        let notify = !didClose
        didClose = true
        lock.unlock()
        if notify {
            callback?(reason)
        }
    }

    var closed: Bool {
        lock.lock()
        defer { lock.unlock() }
        return didClose
    }
}

private func makeStream<Value>(
    owner: SDKClient,
    next: @escaping @Sendable () async throws -> Value?,
    end: @escaping @Sendable () async -> Void,
    connectionState: @escaping @Sendable () -> ConnectionState,
    connectionStateChanged: @escaping @Sendable (ConnectionState) async throws -> ConnectionState,
    onClose: (@Sendable (SDKStreamCloseReason) -> Void)?,
    onConnectionStateChange: (@Sendable (ConnectionState?, ConnectionState) -> Void)?
) -> AsyncThrowingStream<Value, Error> {
    let completion = StreamCompletion(onClose)
    let monitor = Task {
        guard let onConnectionStateChange else { return }
        var previous: ConnectionState?
        func emit(_ current: ConnectionState) {
            guard !completion.closed, previous != current else { return }
            onConnectionStateChange(previous, current)
            previous = current
        }
        emit(.connecting)
        emit(connectionState())
        do {
            while !completion.closed, let last = previous {
                try emit(await connectionStateChanged(last))
            }
        } catch {
            // The pending read reports terminal failure.
        }
    }
    return AsyncThrowingStream<Value, Error>(unfolding: {
        _ = owner.raw
        do {
            let value = try await next()
            if value == nil {
                completion.close(.closed)
                monitor.cancel()
                await end()
            }
            return value
        } catch {
            if !completion.closed {
                completion.close(.failed(error))
            }
            monitor.cancel()
            await end()
            throw error
        }
    }, onCancel: {
        completion.close(.closed)
        monitor.cancel()
        Task { await end() }
    })
}

func makeSDKMessageStream(
    reader: MessageReader,
    owner: SDKClient,
    onClose: (@Sendable (SDKStreamCloseReason) -> Void)?,
    onConnectionStateChange: (@Sendable (ConnectionState?, ConnectionState) -> Void)?
) -> SDKMessageStream {
    makeStream(
        owner: owner,
        next: { try await reader.next() },
        end: { try? await reader.end() },
        connectionState: { reader.connectionState() },
        connectionStateChanged: { try await reader.connectionStateChanged(previous: $0) },
        onClose: onClose,
        onConnectionStateChange: onConnectionStateChange
    )
}

func makeSDKConversationStream(
    reader: ConversationReader,
    owner: SDKClient,
    onClose: (@Sendable (SDKStreamCloseReason) -> Void)?,
    onConnectionStateChange: (@Sendable (ConnectionState?, ConnectionState) -> Void)?
) -> SDKConversationStream {
    makeStream(
        owner: owner,
        next: { try await reader.next() },
        end: { try? await reader.end() },
        connectionState: { reader.connectionState() },
        connectionStateChanged: { try await reader.connectionStateChanged(previous: $0) },
        onClose: onClose,
        onConnectionStateChange: onConnectionStateChange
    )
}
