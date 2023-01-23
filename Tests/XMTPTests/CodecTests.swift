//
//  CodecTests.swift
//
//
//  Created by Pat Nakajima on 12/21/22.
//

import XCTest
@testable import XMTP

struct NumberCodec: ContentCodec {
	typealias T = Double

	var contentType: XMTP.ContentTypeID {
		ContentTypeID(authorityID: "example.com", typeID: "number", versionMajor: 1, versionMinor: 1)
	}

	func encode(content: Double) throws -> XMTP.EncodedContent {
		var encodedContent = EncodedContent()

		encodedContent.type = ContentTypeID(authorityID: "example.com", typeID: "number", versionMajor: 1, versionMinor: 1)
		encodedContent.content = try JSONEncoder().encode(content)

		return encodedContent
	}

	func decode(content: XMTP.EncodedContent) throws -> Double {
		return try JSONDecoder().decode(Double.self, from: content.content)
	}
}

@available(iOS 15, *)
class CodecTests: XCTestCase {
	func testCanRoundTripWithCustomContentType() async throws {
		Client.register(codec: NumberCodec())

		let fixtures = await fixtures()

		let aliceClient = fixtures.aliceClient!
		let aliceConversation = try await aliceClient.conversations.newConversation(with: fixtures.bob.address)

		try await aliceConversation.send(content: 3.14, options: .init(contentType: NumberCodec().contentType))

		let messages = try await aliceConversation.messages()
		XCTAssertEqual(messages.count, 1)

		if messages.count == 1 {
			let content: Double = try messages[0].content()
			XCTAssertEqual(3.14, content)
		}
	}

	func testFallsBackToFallbackContentWhenCannotDecode() async throws {
		let fixtures = await fixtures()

		let aliceClient = fixtures.aliceClient!
		let aliceConversation = try await aliceClient.conversations.newConversation(with: fixtures.bob.address)

		try await aliceConversation.send(content: 3.14, options: .init(contentType: NumberCodec().contentType, contentFallback: "pi"))

		// Remove number codec from registry
		Client.codecRegistry.codecs.removeValue(forKey: NumberCodec().id)

		let messages = try await aliceConversation.messages()
		XCTAssertEqual(messages.count, 1)

		let content: Double? = try? messages[0].content()
		XCTAssertEqual(nil, content)
		XCTAssertEqual("pi", messages[0].fallbackContent)
	}

	func testCompositeCodecOnePart() async throws {
		Client.register(codec: CompositeCodec())

		let fixtures = await fixtures()

		let aliceClient = fixtures.aliceClient!
		let aliceConversation = try await aliceClient.conversations.newConversation(with: fixtures.bob.address)

		let textContent = try TextCodec().encode(content: "hiya")
		let source = DecodedComposite(encodedContent: textContent)
		try await aliceConversation.send(content: source, options: .init(contentType: CompositeCodec().contentType))
		let messages = try await aliceConversation.messages()

		let decoded: DecodedComposite = try messages[0].content()
		XCTAssertEqual("hiya", try decoded.content())
	}

	func testCompositeCodecCanHaveParts() async throws {
		Client.register(codec: CompositeCodec())
		Client.register(codec: NumberCodec())

		let fixtures = await fixtures()

		let aliceClient = fixtures.aliceClient!
		let aliceConversation = try await aliceClient.conversations.newConversation(with: fixtures.bob.address)

		let textContent = try TextCodec().encode(content: "sup")
		let numberContent = try NumberCodec().encode(content: 3.14)

		let source = DecodedComposite(parts: [
			DecodedComposite(encodedContent: textContent),
			DecodedComposite(parts: [
				DecodedComposite(encodedContent: numberContent),
			]),
		])

		try await aliceConversation.send(content: source, options: .init(contentType: CompositeCodec().contentType))
		let messages = try await aliceConversation.messages()

		let decoded: DecodedComposite = try messages[0].content()
		let part1 = decoded.parts[0]
		let part2 = decoded.parts[1].parts[0]
		XCTAssertEqual("sup", try part1.content())
		XCTAssertEqual(3.14, try part2.content())
	}
}
