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

/// Polls `condition` until it is true or `seconds` pass. Returns the last result.
func eventually(seconds: Double, _ condition: () async throws -> Bool) async rethrows -> Bool {
	let deadline = Date().addingTimeInterval(seconds)
	while Date() < deadline {
		if try await condition() {
			return true
		}
		try? await Task.sleep(for: .milliseconds(50))
	}
	return try await condition()
}

/// The result of `operation`, or nil when it does not finish in `seconds`.
/// The operation task is cancelled at the deadline.
func within<T: Sendable>(
	seconds: Double, _ operation: @escaping @Sendable () async throws -> T,
) async throws -> T? {
	let work = Task { try await operation() }
	let timer = Task {
		try? await Task.sleep(for: .seconds(seconds))
		work.cancel()
	}
	defer { timer.cancel() }
	do {
		return try await work.value
	} catch is CancellationError where work.isCancelled {
		return nil
	}
}
