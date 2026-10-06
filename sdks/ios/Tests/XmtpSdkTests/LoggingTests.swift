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
/// rules are tested in Rust (`xmtp_logging/src/sink_queue/tests.rs`). Rust
/// redacts a record before any host sink gets it, and the Swift callback only
/// lifts the fields, so Rust tests the redaction (LOG-010) for the app sink and
/// the native log: `xmtp_sdk/src/logging/sink/tests.rs::client_log_secrets_are_redacted`.
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

		let client = try await SDKClient.create(signer: generateLocalSigner(), options: liveOptions())
		_ = try await client.conversations().createGroup(members: [InboxId]())
		let delivered = await eventually(seconds: 30) { !sink.records.value.isEmpty }
		try await client.end()

		XCTAssertTrue(delivered, "No SDK log reached the Swift sink after \(sink.calls.value) calls")
		XCTAssertGreaterThan(sink.calls.value, 1, "The sink was not called again after it threw")
		let record = try XCTUnwrap(sink.records.value.first)
		XCTAssertFalse(record.target.isEmpty)
		XCTAssertFalse(record.message.isEmpty && record.fields.isEmpty, "The record has no content")
	}
}
