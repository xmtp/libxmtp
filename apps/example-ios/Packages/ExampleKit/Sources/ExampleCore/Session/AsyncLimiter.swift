import Foundation

public actor AsyncLimiter {
	private var available: Int
	private var waiters: [CheckedContinuation<Void, Never>] = []
	public init(_ limit: Int = 4) {
		precondition(limit > 0); available = limit
	}

	private func acquire() async {
		if available > 0 {
			available -= 1
		} else {
			await withCheckedContinuation { waiters.append($0) }
		}
	}

	private func release() {
		if waiters.isEmpty {
			available += 1
		} else {
			waiters.removeFirst().resume()
		}
	}

	public func withPermit<T: Sendable>(_ operation: @Sendable () async throws -> T) async throws -> T {
		await acquire()
		defer { release() }
		try Task.checkCancellation()
		return try await operation()
	}
}
