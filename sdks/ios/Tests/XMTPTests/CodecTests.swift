import XCTest
@testable import XMTPiOS

struct NumberCodec: ContentCodec {
	func shouldPush(content _: Double) throws -> Bool {
		false
	}

	func fallback(content _: Double) throws -> String? {
		"pi"
	}

	typealias T = Double

	var contentType: XMTPiOS.ContentTypeID {
		ContentTypeID(
			authorityID: "example.com", typeID: "number", versionMajor: 1,
			versionMinor: 1,
		)
	}

	func encode(content: Double) throws
		-> XMTPiOS.EncodedContent
	{
		var encodedContent = EncodedContent()

		encodedContent.type = ContentTypeID(
			authorityID: "example.com", typeID: "number", versionMajor: 1,
			versionMinor: 1,
		)
		encodedContent.content = try JSONEncoder().encode(content)

		return encodedContent
	}

	func decode(content: XMTPiOS.EncodedContent) throws
		-> Double
	{
		try JSONDecoder().decode(Double.self, from: content.content)
	}
}

@available(iOS 15, *)
class CodecTests: XCTestCase {
	override func setUp() {
		super.setUp()
		setupLocalEnv()
	}

	func testCanRoundTripWithCustomContentType() async throws {
		let fixtures = try await fixtures()

		let expectedContent = 3.14

		let alixClient = try XCTUnwrap(fixtures.alixClient)
		let alixConversation = try await alixClient.conversations
			.newConversation(with: fixtures.boClient.inboxID)

		Client.register(codec: NumberCodec())

		try await alixConversation.send(
			content: expectedContent,
			options: .init(contentType: NumberCodec().contentType),
		)

		let messages = try await alixConversation.messages()
		XCTAssertEqual(messages.count, 2)

		if messages.count == 2 {
			let content: Double = try messages[0].content()
			XCTAssertEqual(expectedContent, content)
		}

		let messagesV2 = try await alixConversation.enrichedMessages()
		XCTAssertEqual(messagesV2.count, 2)
		if messages.count == 2 {
			let content: Double = try messages[0].content()
			XCTAssertEqual(expectedContent, content)
		}
	}

	func testFallsBackToFallbackContentWhenCannotDecode() async throws {
		let fixtures = try await fixtures()

		let alixClient = try XCTUnwrap(fixtures.alixClient)
		let alixConversation = try await alixClient.conversations
			.newConversation(with: fixtures.boClient.inboxID)

		Client.register(codec: NumberCodec())

		try await alixConversation.send(
			content: 3.14,
			options: .init(contentType: NumberCodec().contentType),
		)

		// Remove number codec from registry
		Client.codecRegistry.removeCodec(for: NumberCodec().id)

		let messages = try await alixConversation.messages()
		XCTAssertEqual(messages.count, 2)

		let content: Double? = try? messages[0].content()
		XCTAssertEqual(nil, content)
		XCTAssertEqual("pi", try messages[0].fallback)
	}

	func testShouldPushForTextCodec() async throws {
		let fixtures = try await fixtures()
		let alixClient = try XCTUnwrap(fixtures.alixClient)
		let alixConversation = try await alixClient.conversations
			.newConversation(with: fixtures.boClient.inboxID)

		// Text codec should have shouldPush = true
		let textCodec = TextCodec()
		let shouldPush = try textCodec.shouldPush(content: "Hello")
		XCTAssertTrue(shouldPush, "TextCodec should have shouldPush = true")
	}

	func testShouldPushForReactionCodec() async throws {
		// Reaction codec should have shouldPush = false
		let reactionCodec = ReactionCodec()
		let reaction = Reaction(
			reference: "messageId",
			action: .added,
			content: "👍",
			schema: .unicode,
		)
		let shouldPush = try reactionCodec.shouldPush(content: reaction)
		XCTAssertFalse(shouldPush, "ReactionCodec should have shouldPush = false")
	}

	func testShouldPushForReadReceiptCodec() async throws {
		// ReadReceipt codec should have shouldPush = false
		let readReceiptCodec = ReadReceiptCodec()
		let readReceipt = ReadReceipt()
		let shouldPush = try readReceiptCodec.shouldPush(content: readReceipt)
		XCTAssertFalse(shouldPush, "ReadReceiptCodec should have shouldPush = false")
	}

	func testShouldPushForCustomCodec() async throws {
		// Custom NumberCodec should have shouldPush = false
		let numberCodec = NumberCodec()
		let shouldPush = try numberCodec.shouldPush(content: 3.14)
		XCTAssertFalse(shouldPush, "NumberCodec should have shouldPush = false")
	}

	func testMessageVisibilityOptionsToFfi() async {
		// Test that MessageVisibilityOptions correctly converts to FfiSendMessageOpts
		let visibilityOptions = MessageVisibilityOptions(shouldPush: true)
		let ffiOpts = visibilityOptions.toFfi()
		XCTAssertTrue(ffiOpts.shouldPush, "FfiSendMessageOpts should have shouldPush = true")

		let visibilityOptionsNoPush = MessageVisibilityOptions(shouldPush: false)
		let ffiOptsNoPush = visibilityOptionsNoPush.toFfi()
		XCTAssertFalse(
			ffiOptsNoPush.shouldPush, "FfiSendMessageOpts should have shouldPush = false",
		)
	}
}
