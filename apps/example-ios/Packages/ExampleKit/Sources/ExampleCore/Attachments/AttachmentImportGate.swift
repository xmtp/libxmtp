import Foundation

/// A transfer keeps its immutable staging context until its importer has completed.
public actor AttachmentImportGate<Context: Sendable> {
	public private(set) var current: Context?
	private var occupied = false
	private struct Waiter {
		let id: UUID
		let continuation: CheckedContinuation<Void, any Error>
	}

	private var waiting: [Waiter] = []
	public init() {}
	public func withContext<Value: Sendable>(_ context: Context,
	                                         operation: @Sendable () async throws -> Value) async throws -> Value
	{
		try await acquire()
		defer { current = nil; release() }
		try Task.checkCancellation()
		current = context
		return try await operation()
	}

	private func acquire() async throws {
		try Task.checkCancellation()
		if !occupied {
			occupied = true; return
		}
		let id = UUID()
		try await withTaskCancellationHandler {
			try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, any Error>) in
				if Task.isCancelled {
					continuation.resume(throwing: CancellationError())
				} else {
					waiting.append(Waiter(id: id, continuation: continuation))
				}
			}
		} onCancel: {
			Task { await self.cancel(id) }
		}
	}

	private func cancel(_ id: UUID) {
		guard let index = waiting.firstIndex(where: { $0.id == id }) else { return }
		waiting.remove(at: index).continuation.resume(throwing: CancellationError())
	}

	private func release() {
		if waiting.isEmpty {
			occupied = false
		} else {
			waiting.removeFirst().continuation.resume()
		}
	}
}
