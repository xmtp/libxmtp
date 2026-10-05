import Foundation
import XCTest
import XmtpSdk

final class RecordCodecTests: XCTestCase {
	func testPublicTimestampKeepsExactNanosecondsInTimeFilters() throws {
		let values: [Int64] = [.min, -1, 0, 1, 9_007_199_254_740_993, .max]
		for ns in values {
			let timestamp = Timestamp(ns: ns)
			XCTAssertEqual(timestamp.ns, ns)
			XCTAssertEqual(FfiConverterTypeTimestamp.lower(timestamp), ns)
			XCTAssertEqual(try FfiConverterTypeTimestamp.lift(ns).ns, ns)
			let messages = ListMessagesOptions(
				sentBefore: timestamp, sentAfter: timestamp,
				insertedBefore: timestamp, insertedAfter: timestamp,
			)
			var messageBytes: [UInt8] = []
			FfiConverterTypeListMessagesOptions.write(messages, into: &messageBytes)
			var messageBuffer = (data: Data(messageBytes), offset: 0)
			let decodedMessages = try FfiConverterTypeListMessagesOptions.read(from: &messageBuffer)
			XCTAssertEqual(decodedMessages.sentBefore?.ns, ns)
			XCTAssertEqual(decodedMessages.sentAfter?.ns, ns)
			XCTAssertEqual(decodedMessages.insertedBefore?.ns, ns)
			XCTAssertEqual(decodedMessages.insertedAfter?.ns, ns)
			let conversations = ListConversationsOptions(
				createdAfter: timestamp, createdBefore: timestamp,
				lastActivityAfter: timestamp, lastActivityBefore: timestamp,
			)
			var conversationBytes: [UInt8] = []
			FfiConverterTypeListConversationsOptions.write(conversations, into: &conversationBytes)
			var conversationBuffer = (data: Data(conversationBytes), offset: 0)
			let decodedConversations = try FfiConverterTypeListConversationsOptions.read(from: &conversationBuffer)
			XCTAssertEqual(decodedConversations.createdAfter?.ns, ns)
			XCTAssertEqual(decodedConversations.createdBefore?.ns, ns)
			XCTAssertEqual(decodedConversations.lastActivityAfter?.ns, ns)
			XCTAssertEqual(decodedConversations.lastActivityBefore?.ns, ns)
		}
	}

	func testRemoteAttachmentLength() throws {
		let codec = RemoteAttachmentCodec()
		var value = RemoteAttachment(
			url: "https://example.com/file", contentDigest: "deadbeef",
			secret: Data(repeating: 1, count: 32), salt: Data(repeating: 2, count: 32),
			nonce: Data(repeating: 3, count: 12), scheme: "https://", contentLength: nil, filename: "file",
		)
		let absent = try codec.encode(value)
		XCTAssertNil(absent.parameters["contentLength"])
		XCTAssertNil(try codec.decode(absent).contentLength)
		value.contentLength = 1234
		let present = try codec.encode(value)
		XCTAssertEqual(present.parameters["contentLength"], "1234")
		XCTAssertEqual(try codec.decode(present), value)
	}

	// Smoke for the host-only init in RemoteAttachmentProjection.swift. The Rust
	// pure_codec_tests.rs::remote_attachment_projection_* tests cover the projection rules.
	func testRemoteAttachmentFromEncryptedContent() throws {
		let encrypted = EncryptedEncodedContent(
			ciphertext: Data([1, 2, 3, 4, 5]),
			keys: EncryptionKeys(
				secret: Data(repeating: 1, count: 32), salt: Data(repeating: 2, count: 32),
				nonce: Data(repeating: 3, count: 12), digest: "wrong", length: 999,
			),
		)
		let remote = try RemoteAttachment(
			url: "https://example.com/file", encryptedEncodedContent: encrypted, filename: "file.bin",
		)
		XCTAssertEqual(remote.url, "https://example.com/file")
		XCTAssertEqual(remote.contentLength, 5)
		XCTAssertEqual(remote.contentDigest, "74f81fe167d99b4cb41d6d0ccda82278caee9f3e2f25d5e5a3936ff3dcec60d0")
		XCTAssertEqual(remote.secret, encrypted.keys.secret)
		XCTAssertEqual(remote.filename, "file.bin")
	}

	func testLeaveRequestEmptyNoteNormalization() throws {
		XCTAssertNil(LeaveRequest().authenticatedNote)
		XCTAssertNil(LeaveRequest(authenticatedNote: Data()).authenticatedNote)
		let note = Data("note".utf8)
		XCTAssertEqual(LeaveRequest(authenticatedNote: note).authenticatedNote, note)
		let codec = LeaveRequestCodec()
		// Protobuf field 1 contains an empty byte string.
		let wire = EncodedContent(type: codec.type, content: Data([0x0A, 0x00]))
		XCTAssertNil(try codec.decode(wire).authenticatedNote)
		XCTAssertEqual(try codec.decode(codec.encode(LeaveRequest(authenticatedNote: note))).authenticatedNote, note)
	}
}
