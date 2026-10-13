import ExampleCore
@testable import ExampleSDK
import Foundation
import Testing
import XmtpSdk

struct ChatSDKMappingTests {
	@Test func pendingPagesReadNewestFirst() {
		let options = pendingMessageOptions()
		#expect(options.direction == .descending)
		#expect(options.limit == 50)
		#expect(options.kind == .application)
		#expect(options.sortBy == .sentAt)
		#expect(historyMessageOptions(ascending: false).kind == .application)
	}

	@Test func mapsTextMarkdownReplyAndUnsupportedFallback() {
		#expect(coreContent(.standard(.text("text")), fallback: nil, quote: nil) == .text("text"))
		#expect(coreContent(.standard(.markdown("**plain**")), fallback: nil, quote: nil) == .markdown("**plain**"))
		#expect(coreContent(.standard(.reply(referenceId: "id", body: .text("body"))), fallback: nil, quote: "quote") ==
			.reply(
				quote: "quote",
				body: "body",
			))
		#expect(coreContent(.standard(.readReceipt), fallback: "fallback", quote: nil) == .unsupported("fallback"))
		#expect(coreContent(.standard(.readReceipt), fallback: nil, quote: nil) == .unsupported("Unsupported content"))
	}

	@Test func mapsDeletedCustomAndUnknownContent() {
		#expect(coreContent(.standard(.deletedMessage(DeletedMessage(deletedBy: .sender))), fallback: nil, quote: nil) ==
			.deleted)
		let encoded = EncodedContent(type: standardContentType(kind: .text), content: Data())
		#expect(coreContent(
			.custom(encoded: encoded, rawBytes: Data(), value: nil, error: nil),
			fallback: "Custom fallback",
			quote: nil,
		) == .unsupported("Custom fallback"))
		let error = ErrorDetails(code: "Unknown", category: .unknown, retryable: false, message: "unknown")
		#expect(coreContent(
			.unknown(encoded: nil, rawBytes: Data(), error: error),
			fallback: "Unknown fallback",
			quote: nil,
		) ==
			.unsupported("Unknown fallback"))
	}

	@Test func mapsAttachmentAndFailureCases() {
		#expect(attachmentMIMEType(filename: "photo.jpeg") == "image/jpeg")
		#expect(attachmentMIMEType(filename: "clip.mov") == "video/quicktime")
		#expect(attachmentMIMEType(filename: "notes.txt") == "text/plain")
		let remote = RemoteAttachment(
			url: "https://example.test/file",
			contentDigest: "digest",
			secret: Data(repeating: 1, count: 32),
			salt: Data(repeating: 2, count: 32),
			nonce: Data(repeating: 3, count: 12),
			scheme: "https",
			contentLength: 7,
			filename: "test.txt",
		)
		#expect(sdkFile(coreFile(remote)) == remote)
		#expect(coreContent(.standard(.remoteAttachment(remote)), fallback: nil, quote: nil) == .attachment(coreFile(remote)))
		#expect(attachmentFailure(AttachmentFailure(
			cause: .digestMismatch,
			credentialKind: nil,
			retryable: false,
			missingScope: false,
			httpStatus: nil,
		)).message == "This file could not be verified.")
		#expect(!attachmentFailure(AttachmentFailure(
			cause: .digestMismatch,
			credentialKind: nil,
			retryable: false,
			missingScope: false,
			httpStatus: nil,
		)).retryable)
		#expect(attachmentFailure(AttachmentFailure(
			cause: .network,
			credentialKind: nil,
			retryable: true,
			missingScope: false,
			httpStatus: nil,
		)).retryable)
	}
}
