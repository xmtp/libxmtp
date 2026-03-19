import XCTest
@testable import XMTPiOS
import XMTPTestHelpers

/// Reproduces message loss caused by a group member sending messages (e.g., read
/// receipts) in response to every received message.
///
/// The send path calls sync_until_intent_resolved → sync_with_conn, which
/// processes all pending incoming messages as a side effect. This races with
/// the stream's process_message for the same incoming messages, causing forward
/// secrecy errors and message loss at the stream layer.
///
/// This matches the Convos assistant pattern: the assistant sends a read receipt
/// for every incoming message, creating a 1:1 ratio of sends to receives that
/// reliably triggers the race.
@available(iOS 16, *)
class ForwardSecrecyReproTests: XCTestCase {
	override func setUp() {
		super.setUp()
		setupLocalEnv()
	}

	/// Reproduces: a member that sends a response for every received message loses
	/// incoming messages via the stream.
	///
	/// Setup:
	/// - alix creates a group with bo and assistant
	/// - assistant streams messages and sends a read receipt for each one
	/// - alix sends 100 messages rapidly
	/// - assistant should receive all 100 via the stream
	///
	/// The send path (read receipt) calls sync_with_conn which races with the
	/// stream, consuming messages before the stream delivers them.
	func testSendingOnEveryReceiveCausesMessageLoss() async throws {
		try TestConfig.skipIfNotRunningLocalNodeTests()

		let apiOpts = ClientOptions.Api(
			env: XMTPEnvironment.local,
			isSecure: XMTPEnvironment.local.isSecure
		)

		let alixClient = try await Client.create(
			account: try PrivateKey.generate(),
			options: ClientOptions(api: apiOpts, dbEncryptionKey: Data((0..<32).map { _ in UInt8.random(in: 0...255) }))
		)

		let assistantClient = try await Client.create(
			account: try PrivateKey.generate(),
			options: ClientOptions(api: apiOpts, dbEncryptionKey: Data((0..<32).map { _ in UInt8.random(in: 0...255) }))
		)

		let boClient = try await Client.create(
			account: try PrivateKey.generate(),
			options: ClientOptions(api: apiOpts, dbEncryptionKey: Data((0..<32).map { _ in UInt8.random(in: 0...255) }))
		)

		// Create group with all three members
		let alixGroup = try await alixClient.conversations.newGroup(
			with: [assistantClient.inboxID, boClient.inboxID]
		)

		// Assistant syncs and finds the group
		try await assistantClient.conversations.sync()
		guard let assistantGroup = try await assistantClient.conversations.findGroup(
			groupId: alixGroup.id
		) else {
			XCTFail("Assistant could not find group")
			return
		}

		// Assistant streams messages and sends a read receipt for each one
		// (this is what the Convos assistant was doing)
		let receivedMessages = TestTranscript()
		let streamTask = Task(priority: .userInitiated) {
			for try await message in assistantGroup.streamMessages() {
				if (try? message.encodedContent.type == ContentTypeText) ?? false {
					let text = (try? message.content() as String) ?? ""
					await receivedMessages.add(text)

					// Send a read receipt in response — this triggers sync_with_conn
					// which races with the stream for subsequent messages
					_ = try? await assistantGroup.send(
						content: ReadReceipt(),
						options: .init(contentType: ContentTypeReadReceipt)
					)
				}
			}
		}

		try await Task.sleep(nanoseconds: 1_000_000_000)

		// Alix sends messages rapidly
		let messageCount = 100
		let sendTask = Task {
			for i in 1...messageCount {
				_ = try? await alixGroup.send(content: "msg-\(i)")
			}
		}

		_ = await sendTask.result
		try await Task.sleep(nanoseconds: 10_000_000_000)

		streamTask.cancel()

		let streamCount = await receivedMessages.messages.count

		// Check DB for comparison
		try await assistantGroup.sync()
		let allMessages = try await assistantGroup.messages()
		let textMessages = allMessages.filter { (try? $0.encodedContent.type == ContentTypeText) ?? false }

		print("=== Send-On-Receive (Read Receipt Pattern) ===")
		print("Messages sent by alix: \(messageCount)")
		print("Messages received by assistant via stream: \(streamCount)")
		print("Messages in assistant DB after sync: \(textMessages.count)")

		XCTAssertEqual(
			streamCount, messageCount,
			"Assistant stream received \(streamCount)/\(messageCount). " +
			"\(messageCount - streamCount) messages lost because the read receipt " +
			"send path (sync_with_conn) raced with the stream's process_message."
		)

		try alixClient.deleteLocalDatabase()
		try assistantClient.deleteLocalDatabase()
		try boClient.deleteLocalDatabase()
	}

	/// Control: same setup but assistant does not send read receipts.
	/// Without the concurrent send, the stream should deliver all messages.
	func testReceiveOnlyDoesNotLoseMessages() async throws {
		try TestConfig.skipIfNotRunningLocalNodeTests()

		let apiOpts = ClientOptions.Api(
			env: XMTPEnvironment.local,
			isSecure: XMTPEnvironment.local.isSecure
		)

		let alixClient = try await Client.create(
			account: try PrivateKey.generate(),
			options: ClientOptions(api: apiOpts, dbEncryptionKey: Data((0..<32).map { _ in UInt8.random(in: 0...255) }))
		)

		let assistantClient = try await Client.create(
			account: try PrivateKey.generate(),
			options: ClientOptions(api: apiOpts, dbEncryptionKey: Data((0..<32).map { _ in UInt8.random(in: 0...255) }))
		)

		let boClient = try await Client.create(
			account: try PrivateKey.generate(),
			options: ClientOptions(api: apiOpts, dbEncryptionKey: Data((0..<32).map { _ in UInt8.random(in: 0...255) }))
		)

		let alixGroup = try await alixClient.conversations.newGroup(
			with: [assistantClient.inboxID, boClient.inboxID]
		)

		try await assistantClient.conversations.sync()
		guard let assistantGroup = try await assistantClient.conversations.findGroup(
			groupId: alixGroup.id
		) else {
			XCTFail("Assistant could not find group")
			return
		}

		// Assistant streams messages but does NOT send anything back
		let receivedMessages = TestTranscript()
		let streamTask = Task(priority: .userInitiated) {
			for try await message in assistantGroup.streamMessages() {
				if (try? message.encodedContent.type == ContentTypeText) ?? false {
					let text = (try? message.content() as String) ?? ""
					await receivedMessages.add(text)
				}
			}
		}

		try await Task.sleep(nanoseconds: 1_000_000_000)

		let messageCount = 100
		let sendTask = Task {
			for i in 1...messageCount {
				_ = try? await alixGroup.send(content: "msg-\(i)")
			}
		}

		_ = await sendTask.result
		try await Task.sleep(nanoseconds: 5_000_000_000)

		streamTask.cancel()

		let streamCount = await receivedMessages.messages.count

		print("=== Receive Only (No Read Receipts) ===")
		print("Messages sent by alix: \(messageCount)")
		print("Messages received by assistant via stream: \(streamCount)")

		XCTAssertEqual(
			streamCount, messageCount,
			"Expected \(messageCount) messages, got \(streamCount)."
		)

		try alixClient.deleteLocalDatabase()
		try assistantClient.deleteLocalDatabase()
		try boClient.deleteLocalDatabase()
	}

	/// Variant: assistant sends a delayed response (like an actual reply) after
	/// processing, rather than an immediate read receipt. Tests whether adding
	/// a small delay between receive and send avoids the race.
	func testDelayedSendOnReceiveDoesNotLoseMessages() async throws {
		try TestConfig.skipIfNotRunningLocalNodeTests()

		let apiOpts = ClientOptions.Api(
			env: XMTPEnvironment.local,
			isSecure: XMTPEnvironment.local.isSecure
		)

		let alixClient = try await Client.create(
			account: try PrivateKey.generate(),
			options: ClientOptions(api: apiOpts, dbEncryptionKey: Data((0..<32).map { _ in UInt8.random(in: 0...255) }))
		)

		let assistantClient = try await Client.create(
			account: try PrivateKey.generate(),
			options: ClientOptions(api: apiOpts, dbEncryptionKey: Data((0..<32).map { _ in UInt8.random(in: 0...255) }))
		)

		let boClient = try await Client.create(
			account: try PrivateKey.generate(),
			options: ClientOptions(api: apiOpts, dbEncryptionKey: Data((0..<32).map { _ in UInt8.random(in: 0...255) }))
		)

		let alixGroup = try await alixClient.conversations.newGroup(
			with: [assistantClient.inboxID, boClient.inboxID]
		)

		try await assistantClient.conversations.sync()
		guard let assistantGroup = try await assistantClient.conversations.findGroup(
			groupId: alixGroup.id
		) else {
			XCTFail("Assistant could not find group")
			return
		}

		let receivedMessages = TestTranscript()
		let streamTask = Task(priority: .userInitiated) {
			for try await message in assistantGroup.streamMessages() {
				if (try? message.encodedContent.type == ContentTypeText) ?? false {
					let text = (try? message.content() as String) ?? ""
					await receivedMessages.add(text)

					// Send a response after a short delay, simulating an assistant
					// that processes the message before responding
					Task {
						try? await Task.sleep(nanoseconds: 500_000_000) // 500ms delay
						_ = try? await assistantGroup.send(
							content: ReadReceipt(),
							options: .init(contentType: ContentTypeReadReceipt)
						)
					}
				}
			}
		}

		try await Task.sleep(nanoseconds: 1_000_000_000)

		let messageCount = 50 // fewer messages since each has a 500ms delay
		let sendTask = Task {
			for i in 1...messageCount {
				_ = try? await alixGroup.send(content: "msg-\(i)")
				try? await Task.sleep(nanoseconds: 50_000_000) // 50ms between sends
			}
		}

		_ = await sendTask.result
		try await Task.sleep(nanoseconds: 30_000_000_000) // wait for delayed receipts

		streamTask.cancel()

		let streamCount = await receivedMessages.messages.count

		print("=== Delayed Send-On-Receive (500ms delay) ===")
		print("Messages sent by alix: \(messageCount)")
		print("Messages received by assistant via stream: \(streamCount)")

		XCTAssertEqual(
			streamCount, messageCount,
			"Expected \(messageCount) messages, got \(streamCount)."
		)

		try alixClient.deleteLocalDatabase()
		try assistantClient.deleteLocalDatabase()
		try boClient.deleteLocalDatabase()
	}
}
