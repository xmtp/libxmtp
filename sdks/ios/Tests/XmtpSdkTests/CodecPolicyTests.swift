import Foundation
import XCTest
import XmtpSdk

private let noteType = ContentTypeId(authorityId: "example.org", typeId: "note", versionMajor: 1, versionMinor: 0)
private let emptyAuthority = ContentTypeId(authorityId: "", typeId: "note", versionMajor: 1, versionMinor: 0)

private struct StepNotAllowed: Error {
	let step: String
}

/// A note codec. Each step can fail, return its own fallback, or change type.
private struct NoteCodec: ContentCodec {
	var failEncode = false
	var failFallback = false
	var failPush = false
	var ownFallback: String?
	var envelopeType = noteType
	var push = true
	var type = noteType

	func encode(_ value: String) throws -> EncodedContent {
		if failEncode {
			throw StepNotAllowed(step: "encode")
		}
		return EncodedContent(type: envelopeType, fallback: ownFallback, content: Data(value.utf8))
	}

	func decode(_ encoded: EncodedContent) throws -> String {
		String(decoding: encoded.content, as: UTF8.self)
	}

	func fallback(_ value: String) throws -> String? {
		if failFallback {
			throw StepNotAllowed(step: "fallback")
		}
		return "a note: \(value)"
	}

	func shouldPush(_: String) throws -> Bool {
		if failPush {
			throw StepNotAllowed(step: "shouldPush")
		}
		return push
	}
}

/// A codec of a catalogue type. Its push hook must not run.
private struct CatalogueTextCodec: ContentCodec {
	var type: ContentTypeId {
		TextCodec().type
	}

	func encode(_ value: String) throws -> EncodedContent {
		try TextCodec().encode(value)
	}

	func decode(_ encoded: EncodedContent) throws -> String {
		try TextCodec().decode(encoded)
	}

	func shouldPush(_: String) throws -> Bool {
		throw StepNotAllowed(step: "shouldPush for a catalogue type")
	}
}

/// A codec that counts reads of its type and can cancel its caller's task.
private final class ProbeCodec: ContentCodec, @unchecked Sendable {
	private let reads = Shared(0)
	let cancelInEncode: Bool
	let cancelInFallback: Bool
	let ownCancellation: Bool

	init(cancelInEncode: Bool = false, cancelInFallback: Bool = false, ownCancellation: Bool = false) {
		self.cancelInEncode = cancelInEncode
		self.cancelInFallback = cancelInFallback
		self.ownCancellation = ownCancellation
	}

	var typeReads: Int {
		reads.value
	}

	var type: ContentTypeId {
		reads.update { $0 += 1 }
		return noteType
	}

	func encode(_ value: String) throws -> EncodedContent {
		if cancelInEncode {
			withUnsafeCurrentTask { $0?.cancel() }
		}
		return EncodedContent(type: noteType, content: Data(value.utf8))
	}

	func decode(_ encoded: EncodedContent) throws -> String {
		String(decoding: encoded.content, as: UTF8.self)
	}

	func fallback(_: String) throws -> String? {
		if ownCancellation {
			// The codec throws its own CancellationError in a live task.
			throw CancellationError()
		}
		if cancelInFallback {
			withUnsafeCurrentTask { $0?.cancel() }
			try Task.checkCancellation()
		}
		return nil
	}
}

/// A Group with no Rust object. It records each envelope and options it receives.
private final class RecordingGroup: Group, @unchecked Sendable {
	let sent = Shared<[(EncodedContent, SendOptions?)]>([])

	init() {
		super.init(noHandle: Group.NoHandle())
	}

	required init(unsafeFromHandle handle: UInt64) {
		super.init(unsafeFromHandle: handle)
	}

	override func send(encoded: EncodedContent, options: SendOptions? = nil) async throws -> MessageId {
		sent.update { $0.append((encoded, options)) }
		return "recorded"
	}

	override func prepareMessage(encoded: EncodedContent, options: SendOptions? = nil) async throws -> MessageId {
		try await send(encoded: encoded, options: options)
	}
}

private func isCodecEncodeFailed(_ error: Error) -> Bool {
	guard case let XmtpError.CodecEncodeFailed(details) = error else { return false }
	return details.code == "CodecEncodeFailed" && details.category == .callback && !details.retryable
}

/// The typed codec send policy (`runtime/SDKCodecPolicy.swift`). These tests
/// need no backend: the recording group captures what reaches the send.
final class CodecPolicyTests: XCTestCase {
	/// A typed send fills the fallback from the codec. An envelope fallback is
	/// kept and its hook is not called.
	// verifies: CTYPE-017, CTYPE-021
	func testTypedSendFillsTheFallbackOnce() async throws {
		let group = RecordingGroup()
		_ = try await group.send(NoteCodec(), value: "typed send")
		_ = try await group.prepareMessage(NoteCodec(), value: "prepared")
		_ = try await group.send(NoteCodec(failFallback: true, ownFallback: "own"), value: "kept")
		XCTAssertEqual(group.sent.value.map(\.0.fallback), ["a note: typed send", "a note: prepared", "own"])
		XCTAssertEqual(group.sent.value.map(\.0.content), ["typed send", "prepared", "kept"].map { Data($0.utf8) })
	}

	/// The codec's push hook decides, unless an explicit option or the
	/// catalogue decides. An explicit option keeps its other fields.
	// verifies: SEND-021
	func testPushHookFeedsTheSendOptions() async throws {
		let group = RecordingGroup()
		_ = try await group.send(NoteCodec(push: false), value: "quiet")
		_ = try await group.prepareMessage(NoteCodec(push: true), value: "loud")
		_ = try await group.send(
			NoteCodec(failPush: true), value: "explicit", options: SendOptions(shouldPush: false, optimistic: true),
		)
		_ = try await group.send(CatalogueTextCodec(), value: "catalogue")
		let options = group.sent.value.map(\.1)
		XCTAssertEqual(options.map { $0?.shouldPush }, [false, true, false, nil])
		XCTAssertEqual(options[2]?.optimistic, true)
	}

	/// A failed step, an envelope of another type, or an empty type ID is
	/// `CodecEncodeFailed`, and nothing reaches the send.
	// verifies: CTYPE-003, CTYPE-007
	func testFailedCodecStepNeverSends() async throws {
		let group = RecordingGroup()
		let failing: [NoteCodec] = [
			NoteCodec(failEncode: true),
			NoteCodec(failFallback: true),
			NoteCodec(failPush: true),
			NoteCodec(envelopeType: TextCodec().type),
			// The codec and its envelope agree; only the empty authority fails.
			NoteCodec(envelopeType: emptyAuthority, type: emptyAuthority),
		]
		for codec in failing {
			for prepare in [false, true] {
				do {
					_ = try await prepare
						? group.prepareMessage(codec, value: "x")
						: group.send(codec, value: "x")
					XCTFail("\(codec) did not fail")
				} catch {
					XCTAssertTrue(isCodecEncodeFailed(error), "\(codec): \(error)")
				}
			}
		}
		XCTAssertEqual(group.sent.value.count, 0, "A failed codec step reached the send")
	}

	/// A send reads the codec type once. A task cancelled during a codec step
	/// stops before the send. A codec's own CancellationError in a live task is
	/// a codec failure.
	func testTypeReadOnceAndCancellationStopsTheSend() async throws {
		let group = RecordingGroup()
		let counted = ProbeCodec()
		_ = try await group.send(counted, value: "counted")
		XCTAssertEqual(counted.typeReads, 1)

		let cancelled = await Task { try await group.send(ProbeCodec(cancelInEncode: true), value: "cancelled") }.result
		guard case let .failure(cancelledError) = cancelled, cancelledError is CancellationError else {
			return XCTFail("A cancelled typed send did not stop: \(cancelled)")
		}
		let checked = await Task { try await group.send(ProbeCodec(cancelInFallback: true), value: "checked") }.result
		guard case let .failure(checkedError) = checked, checkedError is CancellationError else {
			return XCTFail("A hook's CancellationError was not kept: \(checked)")
		}
		let own = await Task { try await group.send(ProbeCodec(ownCancellation: true), value: "own") }.result
		guard case let .failure(ownError) = own, isCodecEncodeFailed(ownError) else {
			return XCTFail("A codec's own CancellationError was not CodecEncodeFailed: \(own)")
		}
		XCTAssertEqual(group.sent.value.count, 1, "A cancelled or failed send reached the send")
	}
}
