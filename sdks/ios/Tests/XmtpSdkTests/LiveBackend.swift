import Foundation
import XmtpSdk

/// The backend of this checkout. The test recipes set `XMTP_BACKEND_URL`.
let liveBackendURL = ProcessInfo.processInfo.environment["XMTP_BACKEND_URL"] ?? "http://localhost:5050"

/// Options for a client on the live backend, without device sync.
func liveOptions(
	url: String = liveBackendURL,
	storage: StorageOptions = StorageOptions(location: .inMemory),
) -> ClientOptions {
	ClientOptions(backend: .options(options: BackendOptions(url: url)), storage: storage, deviceSync: false)
}

/// A failed check that is not an XCTest assertion, for example in a callback.
struct TestFailure: Error, CustomStringConvertible {
	let description: String

	init(_ description: String) {
		self.description = description
	}
}

/// A value that tasks and callbacks share.
final class Shared<Value>: @unchecked Sendable {
	private let lock = NSLock()
	private var stored: Value

	init(_ value: Value) {
		stored = value
	}

	var value: Value {
		lock.lock(); defer { lock.unlock() }; return stored
	}

	func update(_ change: (inout Value) -> Void) {
		lock.lock(); defer { lock.unlock() }; change(&stored)
	}
}

/// Sleeps for `seconds`. `Task.sleep(for:)` needs iOS 16 and macOS 13, above the
/// package minimums, so the tests sleep with `Task.sleep(nanoseconds:)`.
func pause(seconds: Double) async throws {
	try await Task.sleep(nanoseconds: UInt64(seconds * 1_000_000_000))
}

/// Polls `condition` until it is true or `seconds` pass. Returns the last result.
func eventually(seconds: Double, _ condition: () async throws -> Bool) async rethrows -> Bool {
	let deadline = Date().addingTimeInterval(seconds)
	while Date() < deadline {
		if try await condition() {
			return true
		}
		try? await pause(seconds: 0.05)
	}
	return try await condition()
}

/// The result of `operation`, or nil when it does not finish in `seconds`.
func within<T: Sendable>(
	seconds: Double, _ operation: @escaping @Sendable () async throws -> T,
) async throws -> T? {
	try await firstResult(of: { try await operation() }, { try await pause(seconds: seconds); return nil })
}

/// The first result or error of `first` and `second`. Each one runs in its own
/// unstructured task. When one finishes, the helper cancels the other and returns
/// without waiting for it, so an operation that ignores cancellation cannot hang a
/// test, and no task outlives the race for longer than its cancellation takes.
/// When the caller is cancelled, the helper cancels both and throws
/// `CancellationError`. A task group cannot do this: it waits for all of its child tasks.
func firstResult<T: Sendable>(
	of first: @escaping @Sendable () async throws -> T,
	_ second: @escaping @Sendable () async throws -> T,
) async throws -> T {
	let race = Race<T>()
	return try await withTaskCancellationHandler {
		try await withCheckedThrowingContinuation { continuation in
			race.start(continuation, [first, second])
		}
	} onCancel: {
		race.finish(.failure(CancellationError()))
	}
}

/// Resumes a continuation once, with the first result that arrives, and then
/// cancels every task of the race.
private final class Race<T: Sendable>: @unchecked Sendable {
	private let lock = NSLock()
	private var continuation: CheckedContinuation<T, Error>?
	private var tasks: [Task<Void, Never>] = []
	private var finished = false

	/// Starts one task for each operation. When the caller was cancelled before
	/// the start, it resumes at once and starts no task.
	func start(_ continuation: CheckedContinuation<T, Error>, _ operations: [@Sendable () async throws -> T]) {
		lock.lock()
		guard !finished else {
			lock.unlock()
			return continuation.resume(throwing: CancellationError())
		}
		self.continuation = continuation
		// The tasks are made under the lock, so `finish` always sees all of them.
		tasks = operations.map { operation in
			Task {
				do {
					try await self.finish(.success(operation()))
				} catch {
					self.finish(.failure(error))
				}
			}
		}
		lock.unlock()
	}

	/// Keeps only the first result. It marks the race finished before it cancels
	/// the tasks, so a cancelled task cannot win.
	func finish(_ result: Result<T, Error>) {
		lock.lock()
		guard !finished else {
			return lock.unlock()
		}
		finished = true
		let first = continuation
		let losers = tasks
		continuation = nil
		tasks = []
		lock.unlock()
		for task in losers {
			task.cancel()
		}
		first?.resume(with: result)
	}
}
