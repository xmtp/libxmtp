import XCTest

@testable import XMTPiOS

struct NumberCodec: ContentCodec {
	func shouldPush(content: Double) throws -> Bool {
		return false
	}

	func fallback(content: Double) throws -> String? {
		return "pi"
	}

	typealias T = Double

	var contentType: XMTPiOS.ContentTypeID {
		ContentTypeID(
			authorityID: "example.com", typeID: "number", versionMajor: 1,
			versionMinor: 1)
	}

	func encode(content: Double) throws
		-> XMTPiOS.EncodedContent
	{
		var encodedContent = EncodedContent()

		encodedContent.type = ContentTypeID(
			authorityID: "example.com", typeID: "number", versionMajor: 1,
			versionMinor: 1)
		encodedContent.content = try JSONEncoder().encode(content)

		return encodedContent
	}

	func decode(content: XMTPiOS.EncodedContent) throws
		-> Double
	{
		return try JSONDecoder().decode(Double.self, from: content.content)
	}
}

@available(iOS 15, *)
class CodecTests: XCTestCase {
	func testCanRoundTripWithCustomContentType() async throws {
		let fixtures = try await fixtures()

		let alixClient = fixtures.alixClient!
		let alixConversation = try await alixClient.conversations
			.newConversation(with: fixtures.bo.address)

		Client.register(codec: NumberCodec())

		try await alixConversation.send(
			content: 3.14,
			options: .init(contentType: NumberCodec().contentType))

		let messages = try await alixConversation.messages()
		XCTAssertEqual(messages.count, 2)

		if messages.count == 2 {
			let content: Double = try messages[0].content()
			XCTAssertEqual(3.14, content)
		}
	}

	func testFallsBackToFallbackContentWhenCannotDecode() async throws {
		let fixtures = try await fixtures()

		let alixClient = fixtures.alixClient!
		let alixConversation = try await alixClient.conversations
			.newConversation(with: fixtures.bo.address)

		Client.register(codec: NumberCodec())

		try await alixConversation.send(
			content: 3.14,
			options: .init(contentType: NumberCodec().contentType))

		// Remove number codec from registry
		Client.codecRegistry.codecs.removeValue(forKey: NumberCodec().id)

		let messages = try await alixConversation.messages()
		XCTAssertEqual(messages.count, 2)

		let content: Double? = try? messages[0].content()
		XCTAssertEqual(nil, content)
		XCTAssertEqual("pi", try messages[0].fallbackContent)
	}
}
