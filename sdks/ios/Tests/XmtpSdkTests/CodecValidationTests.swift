import Foundation
import XCTest
import XmtpSdk

final class CodecValidationTests: XCTestCase {
	// verifies: CTYPE-014, CTYPE-026
	func testRemoteFieldsAndLoopbackUrls() throws {
		let codec = RemoteAttachmentCodec()
		for url in ["https://example.test/file", "http://localhost/file", "http://127.0.0.1/file", "http://[::1]/file"] {
			let value = RemoteAttachment(
				url: url,
				contentDigest: "abababababababababababababababababababababababababababababababab",
				secret: Data(repeating: 1, count: 32),
				salt: Data(repeating: 2, count: 32),
				nonce: Data(repeating: 3, count: 12),
				scheme: url.hasPrefix("https") ? "https://" : "http://",
				contentLength: nil,
				filename: nil,
			)
			let encoded = try codec.encode(value)
			XCTAssertEqual(try codec.decode(encoded).url, url)
			for field in ["contentDigest", "secret", "salt", "nonce", "scheme"] {
				var missing = encoded
				missing.parameters.removeValue(forKey: field)
				XCTAssertThrowsError(try codec.decode(missing))
			}
		}
	}

	// verifies: CTYPE-030
	func testActionsExpiryRangeOnBothFields() throws {
		let codec = ActionsCodec()
		for (timestamp, valid) in [
			("1677-09-21T00:12:43.145224192Z", true), ("2262-04-11T23:47:16.854775807Z", true),
			("1677-09-21T00:12:43.145224191Z", false), ("2262-04-11T23:47:16.854775808Z", false),
		] {
			for nested in [false, true] {
				var action: [String: Any] = ["id": "one", "label": "One"]
				var value: [String: Any] = ["id": "choices", "description": "Choose"]
				if nested {
					action["expiresAt"] = timestamp
				} else {
					value["expiresAt"] = timestamp
				}
				value["actions"] = [action]
				let encoded = try EncodedContent(type: codec.type, content: JSONSerialization.data(withJSONObject: value))
				if valid {
					let decoded = try codec.decode(encoded)
					XCTAssertEqual(
						(nested ? decoded.actions[0].expiresAt : decoded.expiresAt)?.ns,
						timestamp.hasPrefix("1677") ? Int64.min : Int64.max,
					)
				} else {
					XCTAssertThrowsError(try codec.decode(encoded)) { error in
						guard case let XmtpError.CodecDecodeFailed(details) = error else { return XCTFail("Expected CodecDecodeFailed") }
						XCTAssertEqual(details.code, "CodecDecodeFailed")
					}
				}
			}
		}
		let value = Actions(
			id: "choices",
			description: "Choose",
			actions: [Action(id: "one", label: "One", imageUrl: nil, style: nil, expiresAt: nil)],
			expiresAt: Timestamp(ns: Int64.min),
		)
		XCTAssertThrowsError(try codec.encode(value)) { error in
			guard case let XmtpError.InvalidInput(details) = error else { return XCTFail("Expected InvalidInput") }
			XCTAssertEqual(details.code, "InvalidInput")
			XCTAssertEqual(details.category, .input)
			XCTAssertFalse(details.retryable)
		}
	}

	private func wire(_ content: Data, algorithm: UInt8) throws -> Data {
		var encoded = try TextCodec().encode("")
		encoded.content = content
		return try encodeEncodedContent(content: encoded) + Data([0x28, algorithm])
	}

	// verifies: CTYPE-024, CTYPE-025, CTYPE-026
	func testStandaloneAndNestedDecodingHaveAnInclusiveBound() throws {
		// Each independent gzip member expands to 1 MiB of A bytes.
		let member =
			try XCTUnwrap(
				Data(
					base64Encoded: "H4sIAAAAAAAAE+3BMQEAAADCoGzrX8oQvkABAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAHwGyb72gQAAEAA=",
				),
			)
		let extra = try XCTUnwrap(Data(base64Encoded: "H4sIAAAAAAAAE3MEAIue2dMBAAAA"))
		let exact = (0 ..< 16).reduce(into: Data()) { bytes, _ in bytes.append(member) }
		XCTAssertEqual(try decodeEncodedContent(bytes: wire(exact, algorithm: 1)).content.count, 16_777_216)
		let overflow = try wire(exact + extra, algorithm: 1)
		XCTAssertThrowsError(try decodeEncodedContent(bytes: overflow))
		var reply = try ReplyCodec().encode(ReplyContent(
			reference: String(repeating: "a", count: 64),
			content: TextCodec().encode("body"),
		))
		reply.content = overflow
		XCTAssertThrowsError(try ReplyCodec().decode(reply))
		XCTAssertThrowsError(try decodeEncodedContent(bytes: wire(Data([1]), algorithm: 99)))
	}

	// verifies: CTYPE-009
	func testStandaloneCodecFailuresHaveDistinctTypedOutcomes() throws {
		let text = try TextCodec().encode("body")
		var invalid = text
		invalid.content = Data([0xFF])
		var missing = text
		missing.type.authorityId = "other.example"
		let attempts: [(String, () throws -> Any)] = [
			(
				"InvalidInput",
				{ try ActionsCodec().encode(Actions(id: "choices", description: "Choose", actions: [], expiresAt: nil)) }
			),
			("CodecDecodeFailed", { try decodeStandard(encoded: invalid) }),
			("CodecNotFound", { try decodeStandard(encoded: missing) }),
			("MalformedEnvelope", { try decodeEncodedContent(bytes: Data([0xFF])) }),
		]
		for (code, operation) in attempts {
			do { _ = try operation(); XCTFail("Expected codec failure") }
			catch {
				let details: ErrorDetails
				switch error {
				case let XmtpError.InvalidInput(value): details = value
				case let XmtpError.CodecDecodeFailed(value): details = value
				case let XmtpError.CodecNotFound(value): details = value
				case let XmtpError.MalformedEnvelope(value): details = value
				default: XCTFail("Unexpected codec failure"); continue
				}
				XCTAssertEqual(details.code, code)
				XCTAssertEqual(details.category, .input)
				XCTAssertFalse(details.retryable)
				XCTAssertFalse(details.message.isEmpty)
			}
		}
	}

	// verifies: CTYPE-014, CTYPE-026
	func testLegacySchemesKeepStoredParameters() throws {
		let codec = RemoteAttachmentCodec()
		for (url, bare, canonical) in [
			("https://example.test/file", "https", "https://"),
			("http://localhost/file", "http", "http://"),
		] {
			let value = RemoteAttachment(
				url: url,
				contentDigest: String(repeating: "ab", count: 32),
				secret: Data(repeating: 1, count: 32),
				salt: Data(repeating: 2, count: 32),
				nonce: Data(repeating: 3, count: 12),
				scheme: canonical,
				contentLength: nil,
				filename: nil,
			)
			var legacy = try codec.encode(value)
			legacy.parameters["scheme"] = bare
			let raw = try encodeEncodedContent(content: legacy)
			let original = raw
			let parameters = legacy.parameters
			let decoded = try codec.decode(decodeEncodedContent(bytes: raw))
			XCTAssertEqual(decoded.scheme, canonical)
			XCTAssertEqual(legacy.parameters["scheme"], bare)
			XCTAssertEqual(legacy.parameters, parameters)
			XCTAssertEqual(raw, original)
			XCTAssertEqual(try codec.encode(decoded).parameters["scheme"], canonical)
			var bareInput = value
			bareInput.scheme = bare
			XCTAssertThrowsError(try codec.encode(bareInput))
			if bare == "http" {
				var publicHttp = legacy
				publicHttp.content = Data("http://example.test/file".utf8)
				XCTAssertThrowsError(try codec.decode(publicHttp))
			}
			let reply = try ReplyCodec().encode(ReplyContent(reference: String(repeating: "a", count: 64), content: legacy))
			XCTAssertEqual(try codec.decode(ReplyCodec().decode(reply).content).scheme, canonical)
		}
	}
}
