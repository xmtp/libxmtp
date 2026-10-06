import Foundation
import XCTest
import XmtpSdk

/// The hand-written Swift standard codecs (`runtime/SDKCodecs.swift`). Each
/// codec maps its value to one `StandardContent` variant, and Rust encodes the
/// bytes. Rust tests (`pure_codec_tests.rs`) cover the Rust codecs, not this
/// mapping.
final class StandardCodecTests: XCTestCase {
	private let reference: MessageId = String(repeating: "d", count: 64)
	private let remote = RemoteAttachment(
		url: "https://example.test/file", contentDigest: "digest",
		secret: Data(repeating: 1, count: 32), salt: Data(repeating: 2, count: 32),
		nonce: Data(repeating: 3, count: 12), scheme: "https", contentLength: 10, filename: "file",
	)

	/// The codec writes the same bytes as Rust for `standard`, reads them back
	/// to `value`, and reports the type, fallback, and push choice of those bytes.
	/// A different standard variant is rejected.
	private func check<C: ContentCodec>(
		_ codec: C, _ value: C.Value, _ standard: StandardContent,
		equal: (C.Value, C.Value) -> Bool,
		file: StaticString = #filePath, line: UInt = #line,
	) throws {
		let name = String(describing: C.self)
		let expected = try encodeStandard(value: standard)
		let encoded = try codec.encode(value)
		XCTAssertEqual(encoded, expected, "\(name) bytes differ from Rust", file: file, line: line)
		XCTAssertEqual(
			try decodeStandard(encoded: encoded),
			standard,
			"\(name) encoded another value",
			file: file,
			line: line,
		)
		XCTAssertTrue(try equal(codec.decode(expected), value), "\(name) decoded another value", file: file, line: line)
		XCTAssertEqual(codec.type, expected.type, "\(name) type", file: file, line: line)
		XCTAssertEqual(try codec.fallback(value), expected.fallback, "\(name) fallback", file: file, line: line)
		XCTAssertEqual(
			try codec.shouldPush(value), catalogueContentTypeShouldPush(contentType: expected.type),
			"\(name) push choice", file: file, line: line,
		)
		let other: StandardContent = if case .readReceipt = standard {
			.text("other")
		} else {
			.readReceipt
		}
		XCTAssertThrowsError(
			try codec.decode(encodeStandard(value: other)),
			"\(name) accepted another variant",
			file: file,
			line: line,
		)
	}

	private func check<C: ContentCodec>(
		_ codec: C, _ value: C.Value, _ standard: StandardContent,
		file: StaticString = #filePath, line: UInt = #line,
	) throws where C.Value: Equatable {
		try check(codec, value, standard, equal: ==, file: file, line: line)
	}

	func testValueCodecsMatchRust() throws {
		try check(TextCodec(), "hello", .text("hello"))
		try check(MarkdownCodec(), "**hello**", .markdown("**hello**"))
		try check(ReadReceiptCodec(), (), .readReceipt, equal: { _, _ in true })
		let attachment = Attachment(filename: "file.txt", mimeType: "text/plain", content: Data("file".utf8))
		try check(AttachmentCodec(), attachment, .attachment(attachment))
		try check(RemoteAttachmentCodec(), remote, .remoteAttachment(remote))
		let multi = MultiRemoteAttachment(attachments: [remote])
		try check(MultiRemoteAttachmentCodec(), multi, .multiRemoteAttachment(multi))
		let transaction = TransactionReference(namespace: "eip155", networkId: "1", reference: "0x1", metadata: nil)
		try check(TransactionReferenceCodec(), transaction, .transactionReference(transaction))
		let calls = WalletSendCalls(version: "1", chainId: "0x1", from: "0xsender", calls: [], capabilities: nil)
		try check(WalletSendCallsCodec(), calls, .walletSendCalls(calls))
		let actions = Actions(
			id: "actions", description: "Choose",
			actions: [Action(id: "one", label: "One", imageUrl: nil, style: nil, expiresAt: nil)], expiresAt: nil,
		)
		try check(ActionsCodec(), actions, .actions(actions))
		let intent = Intent(id: "actions", actionId: "one", metadataJson: nil)
		try check(IntentCodec(), intent, .intent(intent))
		let updated = GroupUpdated(
			initiatedByInboxId: "inbox", addedInboxes: ["added"], removedInboxes: [], leftInboxes: [],
			metadataFieldChanges: [], addedAdminInboxes: [], removedAdminInboxes: [],
			addedSuperAdminInboxes: [], removedSuperAdminInboxes: [],
		)
		try check(GroupUpdatedCodec(), updated, .groupUpdated(updated))
		let leave = LeaveRequest(authenticatedNote: Data("note".utf8))
		try check(LeaveRequestCodec(), leave, .leaveRequest(leave))
	}

	/// The reaction, reply, and delete-message codecs build their own records,
	/// so each field, and an absent or present reference inbox, must survive.
	func testRecordCodecsKeepEveryField() throws {
		XCTAssertNil(ReactionV2Content(
			reference: reference,
			reaction: Reaction(content: "x", action: .added, schema: .unicode),
		).referenceInboxId)
		var nested = try TextCodec().encode("nested")
		nested.parameters = ["key": "value"]
		nested.fallback = "nested fallback"
		XCTAssertNil(ReplyContent(reference: reference, content: nested).referenceInboxId)
		for inbox in [nil, String(repeating: "b", count: 64)] as [InboxId?] {
			let reaction = Reaction(content: "👍", action: .removed, schema: .shortcode)
			try check(
				ReactionV2Codec(), ReactionV2Content(reference: reference, referenceInboxId: inbox, reaction: reaction),
				.reaction(reference: reference, referenceInboxId: inbox, reaction: reaction),
			)
			try check(
				ReplyCodec(), ReplyContent(reference: reference, referenceInboxId: inbox, content: nested),
				.reply(reference: reference, referenceInboxId: inbox, content: nested),
			)
		}
		try check(DeleteMessageCodec(), DeleteMessageContent(messageId: reference), .deleteMessage(messageId: reference))
	}
}
