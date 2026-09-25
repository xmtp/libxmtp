import Foundation

public typealias SDKMessageStream = SDKReaderStream<Message>
public typealias SDKConversationStream = SDKReaderStream<Conversation>

public enum SDKStreamCloseReason: @unchecked Sendable {
    case closed
    case failed(Error)
}

private final class StreamCompletion: @unchecked Sendable {
    private let lock = NSLock()
    private var didClose = false
    private let callback: (@Sendable (SDKStreamCloseReason) -> Void)?

    init(_ callback: (@Sendable (SDKStreamCloseReason) -> Void)?) {
        self.callback = callback
    }

    func markClosed() -> Bool {
        lock.lock()
        defer { lock.unlock() }
        if didClose { return false }
        didClose = true
        return true
    }

    func notify(_ reason: SDKStreamCloseReason) {
        callback?(reason)
    }

    var closed: Bool {
        lock.lock()
        defer { lock.unlock() }
        return didClose
    }
}

private final class StreamHandle<Value>: @unchecked Sendable {
    // A live iterator holds the host client until its reader ends.
    let owner: SDKClient
    let next: @Sendable () async throws -> Value?
    let end: @Sendable () async -> Void
    let connectionState: @Sendable () -> ConnectionState
    let connectionStateChanged: @Sendable (ConnectionState) async throws -> ConnectionState

    init(
        owner: SDKClient,
        next: @escaping @Sendable () async throws -> Value?,
        end: @escaping @Sendable () async -> Void,
        connectionState: @escaping @Sendable () -> ConnectionState,
        connectionStateChanged: @escaping @Sendable (ConnectionState) async throws -> ConnectionState
    ) {
        self.owner = owner
        self.next = next
        self.end = end
        self.connectionState = connectionState
        self.connectionStateChanged = connectionStateChanged
    }
}

/// Each iterator opens and owns one durable reader. The sequence owns none.
public struct SDKReaderStream<Value>: AsyncSequence {
    public typealias Element = Value
    public typealias Iterator = SDKReaderIterator<Value>

    private let open: @Sendable () async throws -> StreamHandle<Value>
    private let onClose: (@Sendable (SDKStreamCloseReason) -> Void)?
    private let onConnectionStateChange: (@Sendable (ConnectionState?, ConnectionState) -> Void)?

    fileprivate init(
        open: @escaping @Sendable () async throws -> StreamHandle<Value>,
        onClose: (@Sendable (SDKStreamCloseReason) -> Void)?,
        onConnectionStateChange: (@Sendable (ConnectionState?, ConnectionState) -> Void)?
    ) {
        self.open = open
        self.onClose = onClose
        self.onConnectionStateChange = onConnectionStateChange
    }

    public func makeAsyncIterator() -> Iterator {
        Iterator(open: open, onClose: onClose, onConnectionStateChange: onConnectionStateChange)
    }
}

/// The loop releases this object on break, return, and throw.
public final class SDKReaderIterator<Value>: AsyncIteratorProtocol, @unchecked Sendable {
    private let lock = NSLock()
    private let open: @Sendable () async throws -> StreamHandle<Value>
    private let completion: StreamCompletion
    private let onConnectionStateChange: (@Sendable (ConnectionState?, ConnectionState) -> Void)?
    private var handle: StreamHandle<Value>?
    private var opening = false
    private var stopped = false
    private var waiters: [CheckedContinuation<StreamHandle<Value>, Error>] = []
    private var monitor: Task<Void, Never>?

    fileprivate init(
        open: @escaping @Sendable () async throws -> StreamHandle<Value>,
        onClose: (@Sendable (SDKStreamCloseReason) -> Void)?,
        onConnectionStateChange: (@Sendable (ConnectionState?, ConnectionState) -> Void)?
    ) {
        self.open = open
        self.completion = StreamCompletion(onClose)
        self.onConnectionStateChange = onConnectionStateChange
    }

    deinit { close(.closed) }

    private func acquire() async throws -> StreamHandle<Value> {
        try await withCheckedThrowingContinuation { continuation in
            lock.lock()
            if stopped {
                lock.unlock()
                continuation.resume(throwing: CancellationError())
                return
            }
            if let handle {
                lock.unlock()
                continuation.resume(returning: handle)
                return
            }
            waiters.append(continuation)
            let start = !opening
            opening = true
            lock.unlock()
            if start {
                Task.detached { [self] in
                    do { opened(try await open()) }
                    catch { openFailed(error) }
                }
            }
        }
    }

    private func opened(_ newHandle: StreamHandle<Value>) {
        lock.lock()
        if stopped {
            lock.unlock()
            Task.detached { await newHandle.end() }
            return
        }
        handle = newHandle
        let pending = waiters
        waiters.removeAll()
        lock.unlock()
        startMonitor(newHandle)
        for waiter in pending { waiter.resume(returning: newHandle) }
    }

    private func openFailed(_ error: Error) {
        lock.lock()
        let pending = waiters
        waiters.removeAll()
        let wasStopped = stopped
        lock.unlock()
        for waiter in pending { waiter.resume(throwing: error) }
        if !wasStopped { close(.failed(error)) }
    }

    private func startMonitor(_ currentHandle: StreamHandle<Value>) {
        guard let onConnectionStateChange else { return }
        let completion = completion
        let task = Task.detached {
            var previous: ConnectionState?
            func emit(_ current: ConnectionState) {
                guard !completion.closed, previous != current else { return }
                onConnectionStateChange(previous, current)
                previous = current
            }
            emit(.connecting)
            emit(currentHandle.connectionState())
            do {
                while !completion.closed, let last = previous {
                    try emit(await currentHandle.connectionStateChanged(last))
                }
            } catch {
                // The read path reports terminal failure.
            }
        }
        lock.lock()
        if stopped { task.cancel() }
        else { monitor = task }
        lock.unlock()
    }

    private func close(_ reason: SDKStreamCloseReason) {
        lock.lock()
        if stopped {
            lock.unlock()
            return
        }
        stopped = true
        let currentHandle = handle
        handle = nil
        let pending = waiters
        waiters.removeAll()
        let currentMonitor = monitor
        monitor = nil
        let notify = completion.markClosed()
        lock.unlock()
        currentMonitor?.cancel()
        for waiter in pending { waiter.resume(throwing: CancellationError()) }
        if notify {
            let completion = completion
            Task.detached {
                if let currentHandle { await currentHandle.end() }
                completion.notify(reason)
            }
        }
    }

    public func next() async throws -> Value? {
        try await withTaskCancellationHandler(operation: {
            do {
                let currentHandle = try await acquire()
                if Task.isCancelled { throw CancellationError() }
                _ = currentHandle.owner.raw
                let value = try await currentHandle.next()
                if value == nil { close(.closed) }
                return value
            } catch {
                close(Task.isCancelled ? .closed : .failed(error))
                throw error
            }
        }, onCancel: { close(.closed) })
    }
}

func makeSDKMessageStream(
    group: Group,
    owner: SDKClient,
    onClose: (@Sendable (SDKStreamCloseReason) -> Void)?,
    onConnectionStateChange: (@Sendable (ConnectionState?, ConnectionState) -> Void)?
) -> SDKMessageStream {
    SDKReaderStream(open: { [weak owner] in
        guard let owner else { throw CancellationError() }
        let reader = try await group.messageReader()
        return StreamHandle(
            owner: owner,
            next: { try await reader.next() },
            end: { try? await reader.end() },
            connectionState: { reader.connectionState() },
            connectionStateChanged: { try await reader.connectionStateChanged(previous: $0) }
        )
    }, onClose: onClose, onConnectionStateChange: onConnectionStateChange)
}

func makeSDKConversationStream(
    kind: ConversationKind?,
    owner: SDKClient,
    onClose: (@Sendable (SDKStreamCloseReason) -> Void)?,
    onConnectionStateChange: (@Sendable (ConnectionState?, ConnectionState) -> Void)?
) -> SDKConversationStream {
    SDKReaderStream(open: { [weak owner] in
        guard let owner else { throw CancellationError() }
        let reader = try await owner.raw.conversations().conversationReader(kind: kind)
        return StreamHandle(
            owner: owner,
            next: { try await reader.next() },
            end: { try? await reader.end() },
            connectionState: { reader.connectionState() },
            connectionStateChanged: { try await reader.connectionStateChanged(previous: $0) }
        )
    }, onClose: onClose, onConnectionStateChange: onConnectionStateChange)
}
