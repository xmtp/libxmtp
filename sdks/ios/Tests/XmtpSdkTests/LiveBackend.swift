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
/// At the deadline the helper cancels the operation task and returns without
/// waiting for it, so an operation that ignores cancellation cannot hang a test.
/// A task group cannot do this: it waits for all of its child tasks.
func within<T: Sendable>(
	seconds: Double, _ operation: @escaping @Sendable () async throws -> T,
) async throws -> T? {
	try await withCheckedThrowingContinuation { continuation in
		let race = Race(continuation)
		let work = Task {
			do {
				let value = try await operation()
				race.finish(.success(value))
			} catch {
				race.finish(.failure(error))
			}
		}
		Task {
			try? await pause(seconds: seconds)
			// Finish before the cancel, so the cancelled operation cannot win.
			race.finish(.success(nil))
			work.cancel()
		}
	}
}

/// Resumes a continuation once, with the first result that arrives.
private final class Race<T: Sendable>: @unchecked Sendable {
	private let lock = NSLock()
	private var continuation: CheckedContinuation<T?, Error>?

	init(_ continuation: CheckedContinuation<T?, Error>) {
		self.continuation = continuation
	}

	func finish(_ result: Result<T?, Error>) {
		lock.lock()
		let first = continuation
		continuation = nil
		lock.unlock()
		first?.resume(with: result)
	}
}
