import XCTest
@testable import XmtpSdk

/// Holds the listener that `SDKClient.startListener` registers, and can hold the
/// registration until the test releases it.
private actor Registration {
	private var listener: EventListener?
	private var listenerWaiter: CheckedContinuation<EventListener, Never>?
	private var release: CheckedContinuation<Void, Never>?
	private var released = false
	private var callbacks = 0

	func start(_ value: EventListener, hold: Bool) async -> ListenerId {
		listener = value
		listenerWaiter?.resume(returning: value)
		listenerWaiter = nil
		if hold, !released {
			await withCheckedContinuation { release = $0 }
		}
		return 1
	}

	func registeredListener() async -> EventListener {
		if let listener {
			return listener
		}
		return await withCheckedContinuation { listenerWaiter = $0 }
	}

	func releaseStart() {
		released = true
		release?.resume()
		release = nil
	}

	func callback() {
		callbacks += 1
	}

	func callbackCount() -> Int {
		callbacks
	}
}

/// Client close stops each listener callback, also for a registration that is
/// pending at close or that starts after close.
final class ListenerGateTests: XCTestCase {
	private let filter = EventFilter(kinds: [.conversationForkDetected])
	private let event = ClientEvent.conversationForkDetected(conversationForkDetected: GroupRef(groupId: Data([1])))

	private func makeClient(_ registration: Registration, hold: Bool) -> SDKClient {
		let raw = FakeClient(noHandle: Client.NoHandle())
		raw.listenerStarted = { await registration.start($0, hold: hold) }
		return makeSDKClient(raw)
	}

	func testClientEndStopsPendingListener() async throws {
		let registration = Registration()
		let client = makeClient(registration, hold: true)
		let start = Task { try await client.startListener(filter) { _ in await registration.callback() } }
		let listener = await registration.registeredListener()
		try await listener.onEvent(event: event)
		let beforeEnd = await registration.callbackCount()

		try await client.end()
		try await listener.onEvent(event: event)
		let afterEnd = await registration.callbackCount()
		await registration.releaseStart()
		_ = try await start.value
		try await listener.onEvent(event: event)
		let afterRegistration = await registration.callbackCount()

		XCTAssertEqual(beforeEnd, 1, "The open client did not run the callback")
		XCTAssertEqual(afterEnd, 1, "Client close did not stop the pending listener")
		XCTAssertEqual(afterRegistration, 1, "A registration after close restarted the listener")
	}

	func testListenerStartedAfterClientEndIgnoresEvents() async throws {
		let registration = Registration()
		let client = makeClient(registration, hold: false)
		try await client.end()
		_ = try await client.startListener(filter) { _ in await registration.callback() }
		let listener = await registration.registeredListener()
		try await listener.onEvent(event: event)

		let callbacks = await registration.callbackCount()
		XCTAssertEqual(callbacks, 0, "A listener started after client close ran its callback")
	}

	func testClosedRegistryStopsRegisteredGate() {
		let registry = ListenerGates()
		registry.stopAll()
		let gate = ListenerStartGate()
		registry.registered(2, gate: gate)
		XCTAssertFalse(gate.begin(), "A closed registry admitted a registered listener")
	}
}
