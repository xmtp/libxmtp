import Foundation
import XCTest
import XmtpSdk

private struct NoteCodec: ContentCodec {
	let type = ContentTypeId(authorityId: "example.org", typeId: "note", versionMajor: 1, versionMinor: 0)

	func encode(_ value: String) throws -> EncodedContent {
		EncodedContent(type: type, content: Data(value.utf8))
	}

	func decode(_ encoded: EncodedContent) throws -> String {
		String(decoding: encoded.content, as: UTF8.self)
	}

	func fallback(_ value: String) throws -> String? {
		"a note: \(value)"
	}
}

/// A type ID with "/". Joined with its authority, its key text equals that of
/// `collidingType`.
private struct SlashCodec: ContentCodec {
	let type = ContentTypeId(authorityId: "example.org", typeId: "a/b", versionMajor: 1, versionMinor: 0)

	func encode(_ value: String) throws -> EncodedContent {
		EncodedContent(type: type, content: Data(value.utf8))
	}

	func decode(_: EncodedContent) throws -> String {
		"wrong codec"
	}
}

private let collidingType = ContentTypeId(authorityId: "example.org/a", typeId: "b", versionMajor: 1, versionMinor: 0)

private struct DecodeFailure: Error, CustomStringConvertible {
	var description: String {
		"note decode failed"
	}
}

private struct FailingNoteCodec: ContentCodec {
	let type = NoteCodec().type

	func encode(_ value: String) throws -> EncodedContent {
		try NoteCodec().encode(value)
	}

	func decode(_: EncodedContent) throws -> String {
		throw DecodeFailure()
	}
}

/// The per-client receive registry (`runtime/SDKClient.swift`) and the
/// `Message` content projection (`runtime/SDKTypes.swift`).
final class CustomCodecTests: XCTestCase {
	/// A codec decodes only for the client that registered it. A reply body
	/// decodes with the same registry, and without a codec it stays unknown.
	/// Keys keep authority and type ID apart.
	// verifies: CTYPE-017, CTYPE-027
	func testCodecStaysWithItsClient() async throws {
		let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
		defer { try? FileManager.default.removeItem(at: root) }
		let options = liveOptions(storage: StorageOptions(location: .directory(directory: root.path)))
		try await withClients { scope in
			let signer = await generateLocalSigner()
			let withCodec = try await scope.create(signer: signer, options: options, codecs: [NoteCodec()])
			let identity = withCodec.identity()
			let inboxId = withCodec.inboxId()
			let withoutCodec = try await scope.build(identity: identity, options: options, inboxId: inboxId)
			let slash = try await scope.build(
				identity: identity, options: options, inboxId: inboxId, codecs: [SlashCodec()],
			)
			let group = try await withCodec.conversations.createGroup(members: [InboxId]())
			let noteId = try await group.send(NoteCodec(), value: "codec value")
			guard let parent = try await withCodec.conversations.getMessageById(id: noteId) else {
				return XCTFail("The note was not stored")
			}
			let replyId = try await parent.reply(NoteCodec(), value: "reply value")
			let collidingId = try await group.send(encoded: EncodedContent(type: collidingType, content: Data([1])))

			let decoded = try await withCodec.conversations.getMessageById(id: noteId)
			guard case let .custom(_, _, value, nil)? = decoded?.content else {
				return XCTFail("The registered codec did not decode: \(String(describing: decoded?.content))")
			}
			XCTAssertEqual(value as? String, "codec value")
			let reply = try await withCodec.conversations.getMessageById(id: replyId)
			guard case let .custom(nested, _, replyValue, nil)? = reply?.replyContent else {
				return XCTFail("The reply body did not decode with the client codec")
			}
			XCTAssertEqual(replyValue as? String, "reply value")
			XCTAssertEqual(nested.fallback, "a note: reply value", "A typed reply lost the nested fallback")

			for client in [withoutCodec, slash] {
				let other = try await client.conversations.getMessageById(id: noteId)
				guard case let .unknown(encoded, _, error)? = other?.content else {
					return XCTFail("Another client's codec decoded the note: \(String(describing: other?.content))")
				}
				XCTAssertEqual(encoded?.fallback, "a note: codec value")
				XCTAssertEqual(error.code, "CodecNotFound")
				let otherReply = try await client.conversations.getMessageById(id: replyId)
				guard case let .unknown(nestedEncoded, _, nestedError)? = otherReply?.replyContent else {
					return XCTFail("A reply body without a codec was not unknown")
				}
				XCTAssertEqual(nestedEncoded?.content, Data("reply value".utf8))
				XCTAssertEqual(nestedError.code, "CodecNotFound")
			}
			let colliding = try await slash.conversations.getMessageById(id: collidingId)
			guard case let .unknown(_, _, collisionError)? = colliding?.content else {
				return XCTFail("A codec with another type decoded the message: \(String(describing: colliding?.content))")
			}
			XCTAssertEqual(collisionError.code, "CodecNotFound")
		}
	}

	/// A codec that fails to decode keeps the raw bytes and a typed error, in a
	/// lookup and in a stream. The stream continues with the next message. A
	/// reply whose nested codec fails keeps its outer bytes and fallback.
	// verifies: CTYPE-008, CTYPE-009, CTYPE-029, PROC-045
	func testDecodeFailureKeepsBytesAndTheStream() async throws {
		try await withClients { scope in
			let client = try await scope.create(
				signer: generateLocalSigner(), options: liveOptions(), codecs: [FailingNoteCodec()],
			)
			let group = try await client.conversations.createGroup(members: [InboxId]())
			let badId = try await group.send(NoteCodec(), value: "unreadable")
			let nextId = try await group.sendText(text: "after the failure")
			guard let bad = try await client.conversations.getMessageById(id: badId) else {
				return XCTFail("The failed message was not stored")
			}
			let replyId = try await bad.reply(NoteCodec(), value: "nested")
			guard let reply = try await client.conversations.getMessageById(id: replyId),
			      case let .unknown(outer, outerRaw, outerError) = reply.content
			else { return XCTFail("A failed nested codec did not make the reply unknown") }
			XCTAssertEqual(outerRaw, reply.rawBytes)
			XCTAssertFalse(outerRaw.isEmpty)
			XCTAssertEqual(outer?.fallback, reply.fallback)
			XCTAssertNotNil(reply.fallback)
			XCTAssertEqual(outerError.code, "CodecDecodeFailed")

			let stored = try await client.conversations.getMessageById(id: badId)
			guard let stored, case let .custom(encoded, raw, nil, error?) = stored.content else {
				return XCTFail("A failed decode lost its typed details")
			}
			XCTAssertEqual(encoded.content, Data("unreadable".utf8))
			XCTAssertEqual(raw, stored.rawBytes)
			XCTAssertFalse(raw.isEmpty)
			XCTAssertEqual(error.code, "CodecDecodeFailed")
			XCTAssertEqual(error.category, .callback)
			XCTAssertFalse(error.retryable)
			XCTAssertTrue(error.message.contains("note decode failed"), error.message)

			let iterator = try await group.streamMessages().makeAsyncIterator()
			let first = try await within(seconds: 30) { try await iterator.next() }
			guard case let .custom(_, _, nil, streamError?)? = first??.content else {
				return XCTFail("The stream did not deliver the failed message")
			}
			XCTAssertEqual(first??.id, badId)
			XCTAssertEqual(streamError.code, "CodecDecodeFailed")
			let next = try await within(seconds: 30) { try await iterator.next() }
			guard case .standard(.text("after the failure"))? = next??.content else {
				return XCTFail("A codec failure stopped the stream")
			}
			XCTAssertEqual(next??.id, nextId)
			let afterReply = try await within(seconds: 30) { try await iterator.next() }
			XCTAssertEqual(afterReply??.id, replyId)
		}
	}

	/// A parent whose codec fails does not change the reply: only a failed
	/// reply body makes the outer message unknown (`runtime/SDKTypes.swift`).
	/// The projection also keeps malformed received bytes without invented
	/// metadata, and a parent projected after its client ends keeps its bytes.
	// verifies: CTYPE-008
	func testMessageProjectionKeepsReceivedBytes() async throws {
		try await withClients { scope in
			let client = try await scope.create(
				signer: generateLocalSigner(), options: liveOptions(), codecs: [FailingNoteCodec()],
			)
			let group = try await client.conversations.createGroup(members: [InboxId]())
			let textId = try await group.sendText(text: "template")
			guard let template = try await client.conversations.getMessageById(id: textId) else {
				return XCTFail("The template message was not stored")
			}

			let raw = Data([0xFF, 0x80])
			let encoded = try NoteCodec().encode("parent")
			var data = template.data
			data.content = .text("valid reply")
			data.inReplyTo = ReplyParent(
				id: template.id, senderInboxId: template.senderInboxId, sentAt: template.sentAt,
				kind: template.kind, deliveryStatus: template.deliveryStatus, rawBytes: raw,
				contentType: encoded.type, fallback: nil, encoded: encoded,
				content: .custom(encoded: encoded, rawBytes: raw),
			)
			let failedParent = Message(data: data)
			guard case .standard(.text("valid reply")) = failedParent.content else {
				return XCTFail("A parent that failed to decode changed the reply: \(failedParent.content)")
			}
			guard case let .custom(_, _, nil, decodeError?)? = failedParent.inReplyToContent else {
				return XCTFail("The parent did not keep its decode failure")
			}
			XCTAssertEqual(decodeError.code, "CodecDecodeFailed")
			XCTAssertEqual(decodeError.category, .callback)
			try await client.end()

			data.rawBytes = raw
			data.contentType = nil
			data.encoded = nil
			data.fallback = nil
			data.content = .unknown(
				encoded: nil, rawBytes: raw,
				error: ErrorDetails(code: "MalformedEnvelope", category: .input, retryable: false, message: "invalid protobuf"),
			)
			let malformed = Message(data: data)
			XCTAssertNil(malformed.contentType)
			XCTAssertNil(malformed.encoded)
			guard case let .unknown(nil, preserved, cause) = malformed.content else {
				return XCTFail("Malformed content was not kept as unknown")
			}
			XCTAssertEqual(preserved, raw)
			XCTAssertEqual(cause.code, "MalformedEnvelope")

			data.content = .text("valid reply")
			let reply = Message(data: data)
			guard case .standard(.text("valid reply")) = reply.content else {
				return XCTFail("A parent without a client changed the reply")
			}
			guard case let .custom(_, parentRaw, nil, parentError?)? = reply.inReplyToContent else {
				return XCTFail("The parent lost its bytes or error")
			}
			XCTAssertEqual(parentRaw, raw)
			XCTAssertEqual(parentError.code, "ClientClosed")
		}
	}
}
