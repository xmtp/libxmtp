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

/// A task executor that keeps each job until the test releases it. The
/// generated callback trampoline starts one unstructured task for each native
/// `onEvent` call. A task that prefers this executor is in the same state: the
/// native call has started, and the runtime's `onEvent` has not run yet.
@available(macOS 15.0, iOS 18.0, tvOS 18.0, watchOS 11.0, *)
private final class HeldStartExecutor: TaskExecutor, @unchecked Sendable {
	private let lock = NSLock()
	private let queue = DispatchQueue(label: "ListenerGateTests.HeldStartExecutor")
	private var heldJobs: [UnownedJob] = []
	private var holding = true
	private var heldWaiter: CheckedContinuation<Void, Never>?

	func enqueue(_ job: consuming ExecutorJob) {
		let job = UnownedJob(job)
		lock.lock()
		guard holding else {
			lock.unlock()
			run(job)
			return
		}
		heldJobs.append(job)
		let waiter = heldWaiter
		heldWaiter = nil
		lock.unlock()
		waiter?.resume()
	}

	/// Returns when the first job is held.
	func waitUntilHeld() async {
		await withCheckedContinuation { (continuation: CheckedContinuation<Void, Never>) in
			lock.lock()
			if heldJobs.isEmpty {
				heldWaiter = continuation
				lock.unlock()
			} else {
				lock.unlock()
				continuation.resume()
			}
		}
	}

	func release() {
		lock.lock()
		holding = false
		let jobs = heldJobs
		heldJobs = []
		lock.unlock()
		jobs.forEach(run)
	}

	private func run(_ job: UnownedJob) {
		queue.async { job.runSynchronously(on: self.asUnownedTaskExecutor()) }
	}
}

/// Client close and `stopListener` stop each listener callback, also for a
/// registration that is pending at close or that starts after close.
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

	/// A native call that has started but has not passed the start gate when
	/// `stopListener` returns does not run the callback. The call is held
	/// right before the runtime's `onEvent`, whose first step is the gate. Stop
	/// and client close return while the call is held.
	// verifies: EVENT-053
	func testCallbackHeldAtItsStartDoesNotRunAfterStop() async throws {
		guard #available(macOS 15.0, iOS 18.0, tvOS 18.0, watchOS 11.0, *) else {
			throw XCTSkip("Task executor preference needs macOS 15 or iOS 18")
		}
		let registration = Registration()
		let client = makeClient(registration, hold: false)
		let id = try await client.startListener(filter) { _ in await registration.callback() }
		let listener = await registration.registeredListener()
		try await listener.onEvent(event: event)
		let beforeStop = await registration.callbackCount()

		let stopExecutor = HeldStartExecutor()
		let heldAtStop = heldCall(listener, on: stopExecutor)
		await stopExecutor.waitUntilHeld()
		await client.stopListener(id)
		stopExecutor.release()
		try await heldAtStop.value
		let afterHeldCall = await registration.callbackCount()

		try await listener.onEvent(event: event)
		let afterStop = await registration.callbackCount()

		let endExecutor = HeldStartExecutor()
		let heldAtEnd = heldCall(listener, on: endExecutor)
		await endExecutor.waitUntilHeld()
		try await client.end()
		endExecutor.release()
		try await heldAtEnd.value
		let afterEnd = await registration.callbackCount()

		XCTAssertEqual(beforeStop, 1, "The running listener did not run the callback")
		XCTAssertEqual(afterHeldCall, 1, "A callback held at its start ran after stopListener returned")
		XCTAssertEqual(afterStop, 1, "A callback started after stopListener returned")
		XCTAssertEqual(afterEnd, 1, "A callback held across client close ran")
	}

	/// Starts a native call whose task is held before `onEvent` runs.
	@available(macOS 15.0, iOS 18.0, tvOS 18.0, watchOS 11.0, *)
	private func heldCall(_ listener: EventListener, on executor: HeldStartExecutor) -> Task<Void, Error> {
		let event = event
		return Task(executorPreference: executor) { try await listener.onEvent(event: event) }
	}

	func testClosedRegistryStopsRegisteredGate() {
		let registry = ListenerGates()
		registry.stopAll()
		let gate = ListenerStartGate()
		registry.registered(2, gate: gate)
		XCTAssertFalse(gate.begin(), "A closed registry admitted a registered listener")
	}
}
