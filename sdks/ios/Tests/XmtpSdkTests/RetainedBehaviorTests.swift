import Foundation
import XCTest
import XmtpSdk

final class RetainedBehaviorTests: XCTestCase {
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

	func testLegacyRemoteSchemeDecode() throws {
		let codec = RemoteAttachmentCodec()
		var encoded = try codec.encode(RemoteAttachment(
			url: "https://example.com/file", contentDigest: "deadbeef",
			secret: Data(repeating: 1, count: 32), salt: Data(repeating: 2, count: 32),
			nonce: Data(repeating: 3, count: 12), scheme: "https://", contentLength: nil, filename: nil,
		))
		encoded.parameters["scheme"] = "https"
		XCTAssertEqual(try codec.decode(encoded).scheme, "https")
	}

	func testRemoteAttachmentEncryptedProjection() throws {
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
		XCTAssertEqual(remote.contentLength, 5)
		XCTAssertEqual(remote.contentDigest, "74f81fe167d99b4cb41d6d0ccda82278caee9f3e2f25d5e5a3936ff3dcec60d0")
		XCTAssertEqual(remote.secret, encrypted.keys.secret)
		XCTAssertEqual(remote.salt, encrypted.keys.salt)
		XCTAssertEqual(remote.nonce, encrypted.keys.nonce)
		XCTAssertEqual(remote.filename, "file.bin")
		XCTAssertEqual(remote.url, "https://example.com/file")
		XCTAssertEqual(remote.scheme, "https://")
		let local = try RemoteAttachment(url: "http://127.0.0.1/file", encryptedEncodedContent: encrypted)
		XCTAssertNil(local.filename)
		XCTAssertEqual(local.scheme, "http://")
		XCTAssertEqual(local.contentLength, 5)
		let multiple = MultiRemoteAttachment(attachments: [remote, local])
		let codec = MultiRemoteAttachmentCodec()
		XCTAssertEqual(try codec.decode(codec.encode(multiple)), multiple)
		do {
			_ = try RemoteAttachment(url: "file:///tmp/file", encryptedEncodedContent: encrypted)
			XCTFail("A file URL was accepted as a remote attachment")
		} catch XmtpError.InvalidArgument {}
	}

	func testLeaveRequestCodec() throws {
		let codec = LeaveRequestCodec()
		for note in [nil, Data("note".utf8)] as [Data?] {
			let value = LeaveRequest(authenticatedNote: note)
			XCTAssertEqual(try codec.decode(codec.encode(value)), value)
		}
		XCTAssertEqual(codec.type.authorityId, "xmtp.org")
		XCTAssertEqual(codec.type.typeId, "leave_request")
		XCTAssertEqual(codec.type.versionMajor, 1)
		XCTAssertEqual(codec.type.versionMinor, 0)
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

	func testStandardCodecHooks() throws {
		let leave = LeaveRequest(authenticatedNote: nil)
		XCTAssertFalse(try LeaveRequestCodec().shouldPush(leave))
		XCTAssertEqual(try LeaveRequestCodec().fallback(leave), "A member has requested leaving the group")
		XCTAssertFalse(try ReadReceiptCodec().shouldPush(()))
	}

	func testEncryptionRejectsChangedBytes() async throws {
		let bytes = Data([5, 5, 5])
		var encrypted = try await encryptBytes(bytes: bytes)
		let decoded = try await decryptBytes(ciphertext: encrypted.ciphertext, keys: encrypted.keys)
		XCTAssertEqual(decoded, bytes)
		encrypted.ciphertext[0] ^= 1
		do {
			_ = try await decryptBytes(ciphertext: encrypted.ciphertext, keys: encrypted.keys)
			XCTFail("Changed ciphertext was accepted")
		} catch {}
	}

	#if canImport(UIKit)
		func testManageStreamLifecycleDefaultsOn() {
			XCTAssertTrue(SDKClient.manageStreamLifecycle)
		}
	#endif
}
