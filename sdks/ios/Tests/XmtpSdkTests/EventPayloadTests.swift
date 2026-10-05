import Foundation
import XCTest
import XmtpSdk

final class EventPayloadTests: XCTestCase {
	func testNamedPayloadsRoundTripAcrossNativeBuffer() throws {
		let groupId = Data(repeating: 0xA5, count: 16)
		let messageId = Data(repeating: 0x5A, count: 32)
		let installationKey = Data(repeating: 0xD3, count: 32)
		let attachment = AttachmentRef(
			attachmentKey: "attachment-key",
			url: "https://example.invalid/object?token=payload-token",
			contentDigest: "attachment-digest",
		)
		let failed = AttachmentFailed(
			attachmentKey: attachment.attachmentKey,
			url: attachment.url,
			contentDigest: attachment.contentDigest,
			cause: "http_status",
		)
		let events: [ClientEvent] = [
			.attachmentUploadStarted(attachmentUploadStarted: attachment),
			.attachmentUploadCompleted(attachmentUploadCompleted: attachment),
			.attachmentUploadFailed(attachmentUploadFailed: failed),
			.attachmentDownloadStarted(attachmentDownloadStarted: attachment),
			.attachmentDownloadCompleted(attachmentDownloadCompleted: attachment),
			.attachmentDownloadFailed(attachmentDownloadFailed: failed),
			.attachmentDeleted(attachmentDeleted: attachment),
			.conversationJoined(conversationJoined: ConversationJoined(
				groupId: groupId, conversationType: .group, origin: .created, adderInboxId: nil,
			)),
			.messageReceived(messageReceived: MessageReceived(
				groupId: groupId, messageId: messageId,
				contentType: EventContentTypeId(authorityId: "xmtp.org", typeId: "text", versionMajor: 1),
				senderInboxId: "event-payload-inbox",
			)),
			.identityRegistered(identityRegistered: IdentityRegistered(
				inboxId: "event-payload-inbox", installationKey: installationKey,
			)),
			.hmacKeysUpdated(hmacKeysUpdated: HmacKeysUpdated()),
			.consentChanged(consentChanged: ConsentChanged(
				entityKind: .inbox, entity: "event-payload-inbox", state: .allowed,
			)),
		]
		let filter = EventFilter(
			kinds: [.messageReceived],
			groupIds: [groupId],
			contentTypes: [EventContentTypeId(authorityId: "xmtp.org", typeId: "text", versionMajor: 1)],
			referencesOwnMessages: true,
		)
		XCTAssertEqual(try FfiConverterTypeEventFilter_lift(FfiConverterTypeEventFilter_lower(filter)), filter)
		for event in events {
			let buffer = FfiConverterTypeClientEvent_lower(event)
			XCTAssertEqual(try FfiConverterTypeClientEvent_lift(buffer), event)
		}
	}
}
