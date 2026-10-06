import Foundation
import XCTest
@testable import XmtpSdk

private func cancellationOptions(storage: StorageOptions = StorageOptions(location: .inMemory)) -> ClientOptions {
	ClientOptions(
		backend: .options(options: BackendOptions(
			url: ProcessInfo.processInfo.environment["XMTP_BACKEND_URL"] ?? "http://localhost:5050",
		)),
		storage: storage,
		deviceSync: false,
	)
}

private actor StartGate {
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

/// Runs the jobs of one call's task on a private queue. The test reads when the
/// first job returned: the task then waits at its first suspension. When
/// `holdAfterFirst` is set, the executor keeps every later job until `release()`.
/// This is a test seam only. The shipped bindings do not change.
@available(macOS 15.0, iOS 18.0, *)
private final class CallExecutor: TaskExecutor, @unchecked Sendable {
	private let queue = DispatchQueue(label: "XmtpSdkTests.CallExecutor")
	private let lock = NSLock()
	private let holdAfterFirst: Bool
	private var enqueued = 0
	private var returned = 0
	private var released = false
	private var held: [UnownedJob] = []

	init(holdAfterFirst: Bool = false) {
		self.holdAfterFirst = holdAfterFirst
	}

	var returnedJobs: Int {
		lock.lock()
		defer { lock.unlock() }
		return returned
	}

	var heldJobs: Int {
		lock.lock()
		defer { lock.unlock() }
		return held.count
	}

	func enqueue(_ job: consuming ExecutorJob) {
		let job = UnownedJob(job)
		lock.lock()
		enqueued += 1
		if holdAfterFirst, enqueued > 1, !released {
			held.append(job)
			lock.unlock()
			return
		}
		lock.unlock()
		run(job)
	}

	func release() {
		lock.lock()
		released = true
		let jobs = held
		held.removeAll()
		lock.unlock()
		jobs.forEach(run)
	}

	private func run(_ job: UnownedJob) {
		queue.async {
			job.runSynchronously(on: self.asUnownedTaskExecutor())
			self.lock.lock()
			self.returned += 1
			self.lock.unlock()
		}
	}
}

/// Returns the files in `directory` that this process holds open. A closed
/// store holds no descriptor for its database, WAL or shared-memory file.
private func openFiles(in directory: URL) -> [String] {
	let name = directory.lastPathComponent
	let descriptors = (try? FileManager.default.contentsOfDirectory(atPath: "/dev/fd")) ?? []
	var buffer = [CChar](repeating: 0, count: Int(MAXPATHLEN))
	let paths = descriptors.compactMap(Int32.init).compactMap { descriptor -> String? in
		guard fcntl(descriptor, F_GETPATH, &buffer) != -1 else {
			return nil
		}
		let path = String(cString: buffer)
		return path.contains(name) ? URL(fileURLWithPath: path).lastPathComponent : nil
	}
	return Set(paths).sorted()
}

/// These tests run the generated `uniffiRustCallAsync` glue against real native
/// futures. The Rust tests in `reader_ack_cancellation.rs` and
/// `create_adoption.rs` prove the Rust side. These tests prove that Swift task
/// cancellation reaches the native future and that a cancelled call does not
/// acknowledge, hand off or return a value. They cannot stop a call between its
/// last native poll and its lift: both run in one job, with no seam outside the
/// generated glue.
final class CallerCancellationTests: XCTestCase {
	/// Waits until `condition` is true, for up to 10 seconds.
	private func waitUntil(_ failure: @autoclosure () -> String, _ condition: () -> Bool) async -> Bool {
		let deadline = Date().addingTimeInterval(10)
		while !condition() {
			if Date() > deadline {
				XCTFail(failure())
				return false
			}
			try? await Task.sleep(nanoseconds: 1_000_000)
		}
		return true
	}

	private func settle<Value: Sendable>(
		_ call: Task<Value, Error>,
		_ description: String,
	) async -> Result<Value, Error>? {
		let settled = expectation(description: description)
		let watcher = Task {
			let result = await call.result
			settled.fulfill()
			return result
		}
		guard await XCTWaiter.fulfillment(of: [settled], timeout: 10) == .completed else {
			XCTFail("\(description) did not settle")
			return nil
		}
		return await watcher.value
	}

	private func assertCancelled(_ result: Result<some Sendable, Error>?) {
		guard case let .failure(error)? = result else {
			return XCTFail("The cancelled call returned a value")
		}
		XCTAssertTrue(error is CancellationError, "Unexpected error \(error)")
	}

	/// A read cancelled before its first poll throws and does not acknowledge
	/// the prior item.
	func testReadCancelledBeforeFirstPollKeepsPriorItem() async throws {
		try await withClients { scope in
			let client = try await scope.create(signer: generateLocalSigner(), options: cancellationOptions())
			let group = try await client.conversations().createGroup(members: [InboxId]())
			let first = try await group.sendText(text: "delivered before the cancelled read")
			let reader = try await group.messageReader(options: nil)
			let delivered = try await reader.next()
			XCTAssertEqual(delivered?.id, first)

			let gate = StartGate()
			let call = Task {
				await gate.wait()
				return try await reader.next()
			}
			call.cancel()
			await gate.release()
			let result = await settle(call, "The cancelled read before poll")
			guard result != nil else {
				return
			}
			assertCancelled(result)

			try await reader.end()
			let replay = try await group.messageReader(options: nil)
			let replayed = try await replay.next()
			XCTAssertEqual(replayed?.id, first, "The cancelled read acknowledged the prior item")
			try await replay.end()
		}
	}

	/// Cancelling a pending read cancels the native future. Nothing else wakes
	/// it, because no message arrives. The reader stays open, and its next read
	/// returns the message sent after the cancellation.
	func testCancelledPendingReadEndsTheNativeFutureAndKeepsLaterItem() async throws {
		guard #available(macOS 15.0, iOS 18.0, *) else {
			throw XCTSkip("The call executor needs a task executor")
		}
		try await withClients { scope in
			let client = try await scope.create(signer: generateLocalSigner(), options: cancellationOptions())
			let group = try await client.conversations().createGroup(members: [InboxId]())
			let reader = try await group.messageReader(options: nil)

			// The generated glue checks cancellation before its first suspension.
			// After the first job returns, only the native future cancel can end the read.
			let executor = CallExecutor()
			let call = Task(executorPreference: executor) { try await reader.next() }
			guard await waitUntil("The read did not suspend", { executor.returnedJobs > 0 }) else {
				call.cancel()
				return
			}
			call.cancel()
			let result = await settle(call, "The cancelled pending read")
			guard result != nil else {
				return
			}
			assertCancelled(result)

			let later = try await group.sendText(text: "sent after the cancelled read")
			let next = try await reader.next()
			XCTAssertEqual(next?.id, later, "The cancelled read took a later item")
			try await reader.end()
		}
	}

	/// A cancelled pending event read, with no event queued, ends the reader
	/// and returns nil. The next test covers an event that is ready first.
	func testCancelledPendingEventReadEndsTheReader() async throws {
		guard #available(macOS 15.0, iOS 18.0, *) else {
			throw XCTSkip("The call executor needs a task executor")
		}
		try await withClients { scope in
			let client = try await scope.create(signer: generateLocalSigner(), options: cancellationOptions())
			let reader = try await client.raw.events(filter: EventFilter(kinds: [.conversationForkDetected]))
			let executor = CallExecutor()
			let call = Task(executorPreference: executor) { try await reader.next() }
			guard await waitUntil("The event read did not suspend", { executor.returnedJobs > 0 }) else {
				call.cancel()
				return
			}
			call.cancel()
			guard let result = await settle(call, "The cancelled event read") else {
				return
			}
			switch result {
			case let .success(event):
				XCTAssertNil(event, "An event was handed off after cancellation")
			case let .failure(error):
				XCTFail("A cancelled event read threw \(error)")
			}
			let reopened = try await reader.next()
			XCTAssertNil(reopened, "The ended event reader read again")
		}
	}

	/// An event read cancelled after its native future became ready does not
	/// hand off the ready event. The executor holds the wake from the native
	/// future, so the test cancels after the event is ready and before Swift
	/// polls the result.
	func testEventReadyBeforeCancellationIsNotHandedOff() async throws {
		guard #available(macOS 15.0, iOS 18.0, *) else {
			throw XCTSkip("The call executor needs a task executor")
		}
		try await withClients { scope in
			let client = try await scope.create(signer: generateLocalSigner(), options: cancellationOptions())
			let reader = try await client.raw.events(filter: EventFilter(kinds: [.conversationJoined]))
			let executor = CallExecutor(holdAfterFirst: true)
			let call = Task(executorPreference: executor) { try await reader.next() }
			guard await waitUntil("The event read did not suspend", { executor.returnedJobs > 0 }) else {
				call.cancel()
				executor.release()
				return
			}
			_ = try await client.conversations().createGroup(members: [InboxId]())
			guard await waitUntil("The event read did not become ready", { executor.heldJobs > 0 }) else {
				call.cancel()
				executor.release()
				return
			}
			call.cancel()
			executor.release()
			guard let result = await settle(call, "The cancelled ready event read") else {
				return
			}
			switch result {
			case let .success(event):
				XCTAssertNil(event, "A ready event was handed off after cancellation")
			case let .failure(error):
				XCTFail("A cancelled event read threw \(error)")
			}
		}
	}

	/// A create cancelled after its native work finished does not return the
	/// client, and the client's store closes.
	func testCreateCancelledAfterNativeWorkClosesTheStore() async throws {
		try await assertConstructorCancelledAfterNativeWorkClosesTheStore(build: false)
	}

	/// A build cancelled after its native work finished does not return the
	/// client, and the client's store closes.
	func testBuildCancelledAfterNativeWorkClosesTheStore() async throws {
		try await assertConstructorCancelledAfterNativeWorkClosesTheStore(build: true)
	}

	/// The native constructor runs on a runtime task, and its future wakes once,
	/// when that task returns the client. The executor holds that wake, so the
	/// test cancels after the native work and before Swift polls the result.
	private func assertConstructorCancelledAfterNativeWorkClosesTheStore(build: Bool) async throws {
		guard #available(macOS 15.0, iOS 18.0, *) else {
			throw XCTSkip("The call executor needs a task executor")
		}
		let directory = FileManager.default.temporaryDirectory
			.appendingPathComponent("xmtp-cancelled-constructor-\(UUID().uuidString)")
		try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
		defer { _ = try? FileManager.default.removeItem(at: directory) }
		let options = cancellationOptions(storage: StorageOptions(location: .explicit(
			dbPath: directory.appendingPathComponent("client.db3").path,
			attachmentsDir: directory.appendingPathComponent("attachments").path,
		)))
		let signer = await generateLocalSigner()
		let identity = try await signer.identity()
		if build {
			try await Client.create(signer: signer, options: options).end()
		}

		let executor = CallExecutor(holdAfterFirst: true)
		let call = Task(executorPreference: executor) { () async throws -> Client in
			if build {
				return try await Client.build(identity: identity, options: options, inboxId: nil)
			}
			return try await Client.create(signer: signer, options: options)
		}
		guard await waitUntil("The native constructor did not finish", { executor.heldJobs > 0 }) else {
			call.cancel()
			executor.release()
			if case let .success(client)? = await settle(call, "The cancelled unfinished constructor") {
				try await client.end()
			}
			return
		}
		XCTAssertFalse(openFiles(in: directory).isEmpty, "The finished constructor has no open store")
		call.cancel()
		executor.release()
		let result = await settle(call, "The cancelled constructor")
		guard result != nil else {
			return
		}
		if case let .success(client)? = result {
			try await client.end()
		}
		assertCancelled(result)
		_ = await waitUntil("The cancelled constructor left its store open: \(openFiles(in: directory))") {
			openFiles(in: directory).isEmpty
		}
	}
}
