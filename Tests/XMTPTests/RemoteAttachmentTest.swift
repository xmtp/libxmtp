//
//  RemoteAttachmentTests.swift
//
//
//  Created by Pat on 2/14/23.
//
import Foundation

import XCTest
@testable import XMTP

// Fakes HTTPS urls
struct TestFetcher: RemoteContentFetcher {
	func fetch(_ url: String) async throws -> Data {
		guard let localURL = URL(string: url.replacingOccurrences(of: "https://", with: "file://")) else {
			throw RemoteAttachmentError.invalidURL
		}

		return try Data(contentsOf: localURL)
	}
}

@available(macOS 13.0, *)
@available(iOS 16, *)
class RemoteAttachmentTests: XCTestCase {
	var iconData: Data!

	override func setUp() async throws {
		// swiftlint:disable force_try
		iconData = Data(base64Encoded: Data("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".utf8))!
	}

	func testBasic() async throws {
		let fixtures = await fixtures()

		fixtures.aliceClient.register(codec: AttachmentCodec())
		fixtures.aliceClient.register(codec: RemoteAttachmentCodec())

		let conversation = try await fixtures.aliceClient.conversations.newConversation(with: fixtures.bobClient.address)
		let enecryptedEncodedContent = try RemoteAttachment.encodeEncrypted(content: "Hello", codec: TextCodec(), with: fixtures.aliceClient)
		var remoteAttachmentContent = try RemoteAttachment(url: "https://example.com", encryptedEncodedContent: enecryptedEncodedContent)
		remoteAttachmentContent.filename = "hello.txt"
		remoteAttachmentContent.contentLength = 5

		_ = try await conversation.send(content: remoteAttachmentContent, options: .init(contentType: ContentTypeRemoteAttachment))
	}

	func testCanUseAttachmentCodec() async throws {
		let fixtures = await fixtures()
		guard case let .v2(conversation) = try await fixtures.aliceClient.conversations.newConversation(with: fixtures.bobClient.address) else {
			XCTFail("no v2 convo")
			return
		}

		fixtures.aliceClient.register(codec: AttachmentCodec())
		fixtures.aliceClient.register(codec: RemoteAttachmentCodec())

		let encryptedEncodedContent = try RemoteAttachment.encodeEncrypted(
			content: Attachment(filename: "icon.png", mimeType: "image/png", data: iconData),
			codec: AttachmentCodec(),
			with: fixtures.aliceClient
		)

		let tempFileURL = URL.temporaryDirectory.appendingPathComponent(UUID().uuidString)
		try encryptedEncodedContent.payload.write(to: tempFileURL)

		// We only allow https:// urls for remote attachments, but it didn't seem worthwhile to spin up a local web server
		// for this, so we use the TestFetcher to swap the protocols
		let fakeHTTPSFileURL = URL(string: tempFileURL.absoluteString.replacingOccurrences(of: "file://", with: "https://"))!
		var content = try RemoteAttachment(url: fakeHTTPSFileURL.absoluteString, encryptedEncodedContent: encryptedEncodedContent)
		content.filename = "icon.png"
		content.contentLength = 123
		content.fetcher = TestFetcher()

		try await conversation.send(content: content, options: .init(contentType: ContentTypeRemoteAttachment))
		let messages = try await conversation.messages()

		XCTAssertEqual(1, messages.count)

		let receivedMessage = messages[0]
		var remoteAttachment: RemoteAttachment = try receivedMessage.content()

		XCTAssertEqual(123, remoteAttachment.contentLength)
		XCTAssertEqual("icon.png", remoteAttachment.filename)

		remoteAttachment.fetcher = TestFetcher()

		let encodedContent: EncodedContent = try await remoteAttachment.content()
		let attachment: Attachment = try encodedContent.decoded(with: fixtures.aliceClient)

		XCTAssertEqual("icon.png", attachment.filename)
		XCTAssertEqual("image/png", attachment.mimeType)

		XCTAssertEqual(iconData, attachment.data)
	}

	func testCannotUseNonHTTPSUrl() async throws {
		let fixtures = await fixtures()
		guard case let .v2(conversation) = try await fixtures.aliceClient.conversations.newConversation(with: fixtures.bobClient.address) else {
			XCTFail("no v2 convo")
			return
		}

		fixtures.aliceClient.register(codec: AttachmentCodec())
		fixtures.aliceClient.register(codec: RemoteAttachmentCodec())

		let encryptedEncodedContent = try RemoteAttachment.encodeEncrypted(
			content: Attachment(filename: "icon.png", mimeType: "image/png", data: iconData),
			codec: AttachmentCodec(),
			with: fixtures.aliceClient
		)

		let tempFileURL = URL.temporaryDirectory.appendingPathComponent(UUID().uuidString)
		try encryptedEncodedContent.payload.write(to: tempFileURL)

		XCTAssertThrowsError(try RemoteAttachment(url: tempFileURL.absoluteString, encryptedEncodedContent: encryptedEncodedContent)) { error in
			switch error as! RemoteAttachmentError {
			case let .invalidScheme(message):
				XCTAssertEqual(message, "scheme must be https")
			default:
				XCTFail("did not raise correct error")
			}
		}
	}

	func testVerifiesContentDigest() async throws {
		let fixtures = await fixtures()
		guard case let .v2(_) = try await fixtures.aliceClient.conversations.newConversation(with: fixtures.bobClient.address) else {
			XCTFail("no v2 convo")
			return
		}

		let encryptedEncodedContent = try RemoteAttachment.encodeEncrypted(
			content: Attachment(filename: "icon.png", mimeType: "image/png", data: iconData),
			codec: AttachmentCodec(),
			with: fixtures.aliceClient
		)

		let tempFileURL = URL.temporaryDirectory.appendingPathComponent(UUID().uuidString)
		try encryptedEncodedContent.payload.write(to: tempFileURL)
		let fakeHTTPSFileURL = URL(string: tempFileURL.absoluteString.replacingOccurrences(of: "file://", with: "https://"))!
		var remoteAttachment = try RemoteAttachment(url: fakeHTTPSFileURL.absoluteString, encryptedEncodedContent: encryptedEncodedContent)
		remoteAttachment.fetcher = TestFetcher()
		let expect = expectation(description: "raised error")

		// Tamper with content
		try Data([1, 2, 3, 4, 5]).write(to: tempFileURL)

		do {
			_ = try await remoteAttachment.content()
		} catch {
			if let error = error as? RemoteAttachmentError, case let .invalidDigest(message) = error {
				XCTAssert(message.hasPrefix("content digest does not match"))
				expect.fulfill()
			}
		}

		wait(for: [expect], timeout: 3)
	}
}
