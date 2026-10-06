import Foundation
import XCTest
import XmtpSdk

private func reentrancyOptions() -> ClientOptions {
	ClientOptions(
		backend: .options(options: BackendOptions(
			url: ProcessInfo.processInfo.environment["XMTP_BACKEND_URL"] ?? "http://localhost:5050",
		)),
		storage: StorageOptions(location: .inMemory),
		deviceSync: false,
	)
}

/// Holds the callback until the test releases it.
private actor CallbackGate {
	private var open = false
	private var waiting: [CheckedContinuation<Void, Never>] = []

	func wait() async {
		if open {
			return
		}
		await withCheckedContinuation { waiting.append($0) }
	}

	func release() {
		open = true
		for waiter in waiting {
			waiter.resume()
		}
		waiting.removeAll()
	}
}

/// Thread-safe values that the callback shares with the test.
private final class CallbackState: @unchecked Sendable {
	private let lock = NSLock()
	private var id: ListenerId?
	private var entered = false
	private var endError: Error?

	var listenerId: ListenerId? {
		get { lock.lock(); defer { lock.unlock() }; return id }
		set { lock.lock(); id = newValue; lock.unlock() }
	}

	var error: Error? {
		get { lock.lock(); defer { lock.unlock() }; return endError }
		set { lock.lock(); endError = newValue; lock.unlock() }
	}

	/// Only the first callback runs the calls.
	func enter() -> Bool {
		lock.lock()
		defer { lock.unlock() }
		guard !entered else { return false }
		entered = true
		return true
	}
}

/// The generated callback boundary must let a running callback call back into
/// its own client. Rust proves the native side in `event_listeners.rs`.
final class CallbackReentrancyTests: XCTestCase {
	// verifies: EVENT-052
	func testHeldCallbackStopsItsListenerAndEndsTheClient() async throws {
		let client = try await SDKClient.create(signer: generateLocalSigner(), options: reentrancyOptions())
		let state = CallbackState()
		let gate = CallbackGate()
		let stopped = expectation(description: "stopListener inside the held callback returns")
		let ended = expectation(description: "Client.end inside the held callback returns")
		let returned = expectation(description: "The held callback returns")
		let id = try await client.startListener(EventFilter(kinds: [.conversationJoined])) { _ in
			guard state.enter() else { return }
			defer { returned.fulfill() }
			if let id = state.listenerId {
				await client.stopListener(id)
			} else {
				XCTFail("The callback ran before startListener returned its ID")
			}
			stopped.fulfill()
			do {
				try await client.end()
			} catch {
				state.error = error
			}
			ended.fulfill()
			await gate.wait()
		}
		state.listenerId = id
		do {
			_ = try await client.conversations().createGroup(members: [InboxId]())
		} catch {
			// Client.end inside the callback can close this call.
		}

		// The gate is still closed, so each call returned while the callback ran.
		let calls = await XCTWaiter.fulfillment(of: [stopped, ended], timeout: 10, enforceOrder: true)
		XCTAssertEqual(calls, .completed, "A call inside the callback waited for the callback to return")
		await gate.release()
		let callback = await XCTWaiter.fulfillment(of: [returned], timeout: 10)
		XCTAssertEqual(callback, .completed, "The released callback did not return")
		XCTAssertNil(state.error, "Client.end inside the callback failed")
		try? await client.end()
	}
}
