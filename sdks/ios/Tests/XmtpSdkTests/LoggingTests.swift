import Foundation
import XCTest
import XmtpSdk

/// Records what the SDK hands to the app sink. The first call throws.
private final class RecordingSink: LogSink, @unchecked Sendable {
	let calls = Shared(0)
	let records = Shared<[LogRecord]>([])

	func log(record: LogRecord) async throws {
		var call = 0
		calls.update { $0 += 1; call = $0 }
		if call == 1 {
			throw LogSinkError.Failed(reason: "deliberate rejection")
		}
		records.update { $0.append(record) }
	}
}

/// The Swift `LogSink` callback path. Bindgen patches the generated callback
/// so that it asks Rust for admission (`sdkLogSinkHandoff`) before it calls
/// the app sink (`apps/xmtp_sdk_bindgen/src/logging_admission.rs`). The queue
/// rules are tested in Rust (`xmtp_logging/src/sink_queue/tests.rs`). The Swift
/// callback forwards record fields unchanged. A Rust test checks that the
/// credential, signing-key and database-key failures it exercises reach the app
/// sink and the native log without those secrets (LOG-010):
/// `xmtp_sdk/src/logging/sink/tests.rs::client_log_secrets_are_redacted`.
final class LoggingTests: XCTestCase {
	override func tearDown() async throws {
		try await setLogSink(sink: nil)
		try await initLogging(options: LoggingOptions(level: .warn))
	}

	/// SDK activity reaches a Swift sink through the admission check, and a
	/// sink that throws does not stop later delivery.
	// verifies: LOG-009
	func testSwiftSinkReceivesSdkLogs() async throws {
		try await initLogging(options: LoggingOptions(level: .debug))
		let sink = RecordingSink()
		try await setLogSink(sink: sink)

		let delivered = try await withClients { scope in
			let client = try await scope.create(signer: generateLocalSigner())
			_ = try await client.conversations.createGroup(members: [InboxId]())
			return await eventually(seconds: 30) { !sink.records.value.isEmpty }
		}

		XCTAssertTrue(delivered, "No SDK log reached the Swift sink after \(sink.calls.value) calls")
		XCTAssertGreaterThan(sink.calls.value, 1, "The sink was not called again after it threw")
		let record = try XCTUnwrap(sink.records.value.first)
		XCTAssertFalse(record.target.isEmpty)
		XCTAssertFalse(record.message.isEmpty && record.fields.isEmpty, "The record has no content")
	}
}

private final class ContractLogMailbox<Value: Sendable>: @unchecked Sendable {
	private let values = Shared<[Value]>([])
	func push(_ value: Value) {
		values.update { $0.append(value) }
	}

	func next() async throws -> Value {
		let ready = await eventually(seconds: 30) { !self.values.value.isEmpty }
		XCTAssertTrue(ready, "Log callback did not reach its barrier")
		var value: Value?
		values.update {
			if !$0.isEmpty {
				value = $0.removeFirst()
			}
		}
		return try XCTUnwrap(value)
	}
}

private final class ContractLogReplies: @unchecked Sendable {
	private let stream: AsyncStream<Bool>
	private let continuation: AsyncStream<Bool>.Continuation
	init() {
		var continuation: AsyncStream<Bool>.Continuation!
		stream = AsyncStream { continuation = $0 }
		self.continuation = continuation
	}

	func push(_ value: Bool) {
		continuation.yield(value)
	}

	func finish() {
		continuation.finish()
	}

	func next() async throws -> Bool {
		var iterator = stream.makeAsyncIterator()
		guard let value = await iterator.next() else {
			throw LogSinkError.Failed(reason: "log barrier cancelled")
		}
		return value
	}
}

private final class ContractLogSink: LogSink, @unchecked Sendable {
	let body: @Sendable (LogRecord) async throws -> Void
	init(_ body: @escaping @Sendable (LogRecord) async throws -> Void) {
		self.body = body
	}

	func log(record: LogRecord) async throws {
		try await body(record)
	}
}

private func emitContractLogs(_ count: Int) async {
	for index in 0 ..< count {
		do {
			if index % 2 == 0 {
				_ = try await localSignerFromPrivateKey(key: Data(repeating: 0, count: 31))
			} else {
				_ = try generateInboxId(identity: PublicIdentity(identifier: "invalid", kind: .ethereum))
			}
			XCTFail("The invalid input was accepted")
		} catch {}
	}
}

extension LoggingTests {
	// verifies: LOG-002, LOG-003, LOG-004, LOG-005, LOG-007, LOG-011, LOG-012, LOG-013
	func testGeneratedSwiftCallbacksKeepQueueOrderBoundAndGeneration() async throws {
		for oldSuccess in [false, true] {
			let records = ContractLogMailbox<LogRecord>()
			let replies = ContractLogReplies()
			defer { replies.finish() }
			let active = Shared(0)
			let maximum = Shared(0)
			try await initLogging(options: LoggingOptions(level: .error))
			try await setLogSink(sink: ContractLogSink { record in
				var now = 0
				active.update { $0 += 1; now = $0 }
				maximum.update { $0 = max($0, now) }
				records.push(record)
				defer { active.update { $0 -= 1 } }
				if try await !(replies.next()) {
					throw LogSinkError.Failed(reason: "test failure")
				}
			})
			await emitContractLogs(1)
			let first = try await records.next()
			XCTAssertEqual(first.droppedRecords, 0)
			await emitContractLogs(4099)
			replies.push(true)
			let second = try await records.next()
			XCTAssertEqual(second.droppedRecords, 3)
			XCTAssertTrue(second.fields["error", default: ""].contains("invalid local signer private key"))
			await emitContractLogs(3)
			replies.push(false)
			let third = try await records.next()
			XCTAssertEqual(third.droppedRecords, 5)
			XCTAssertTrue(third.fields["error", default: ""].contains("invalid public identity"))
			await emitContractLogs(2)
			replies.push(true)
			let fourth = try await records.next()
			XCTAssertEqual(fourth.droppedRecords, 1)
			XCTAssertEqual(maximum.value, 1)
			let replacement = ContractLogMailbox<LogRecord>()
			let generations = ContractLogMailbox<Int>()
			for generation in 1 ... 4 {
				try await setLogSink(sink: nil)
				try await setLogSink(sink: ContractLogSink { record in
					XCTAssertEqual(active.value, 0)
					generations.push(generation)
					replacement.push(record)
				})
				await emitContractLogs(4099)
			}
			XCTAssertEqual(active.value, 1)
			replies.push(oldSuccess)
			let lastGeneration = try await generations.next()
			let last = try await replacement.next()
			XCTAssertEqual(lastGeneration, 4)
			XCTAssertEqual(last.droppedRecords, 3)
			XCTAssertTrue(last.fields["error", default: ""].contains("invalid local signer private key"))
			try await setLogSink(sink: nil)
		}
	}
}
