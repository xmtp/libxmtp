import Foundation

public actor AsyncLimiter {
	private struct Waiter {
		let id: UUID
		let continuation: CheckedContinuation<Void, any Error>
	}

	private var available: Int
	private var waiters: [Waiter] = []
	public init(_ limit: Int = 4) {
		precondition(limit > 0); available = limit
	}

	private func acquire() async throws {
		try Task.checkCancellation()
		if available > 0 {
			available -= 1
			return
		}
		let id = UUID()
		try await withTaskCancellationHandler {
			try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, any Error>) in
				if Task.isCancelled {
					continuation.resume(throwing: CancellationError())
				} else {
					waiters.append(Waiter(id: id, continuation: continuation))
				}
			}
		} onCancel: {
			Task { await self.cancel(id) }
		}
	}

	private func cancel(_ id: UUID) {
		guard let index = waiters.firstIndex(where: { $0.id == id }) else { return }
		waiters.remove(at: index).continuation.resume(throwing: CancellationError())
	}

	private func release() {
		if waiters.isEmpty {
			available += 1
		} else {
			waiters.removeFirst().continuation.resume()
		}
	}

	public func withPermit<T: Sendable>(_ operation: @Sendable () async throws -> T) async throws -> T {
		try await acquire()
		defer { release() }
		try Task.checkCancellation()
		return try await operation()
	}
}
