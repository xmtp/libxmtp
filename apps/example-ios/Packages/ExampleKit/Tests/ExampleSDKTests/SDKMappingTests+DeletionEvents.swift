import ExampleCore
@testable import ExampleSDK
import Foundation
import Testing
import XmtpSdk

struct SDKDeletionEventTests {
	@Test func deletionEventIsSubscribed() {
		#expect(sessionEventKinds.contains(.messageDeleted))
	}

	@Test(arguments: [DeletionCause.deleted, .deletedLocally])
	func deletionPayloadMapsToConversationProjection(cause: DeletionCause) throws {
		let event = ClientEvent.messageDeleted(messageDeleted: MessageDeleted(
			groupId: Data([0xAB, 0x01]),
			messageId: Data([0x00, 0xFE]),
			cause: cause,
		))
		guard case let .messageDeleted(conversationID, messageID, locallyDeleted) = try #require(sessionEvent(event)) else {
			Issue.record("Deletion must project the affected conversation and message")
			return
		}
		#expect(conversationID == "ab01")
		#expect(messageID == "00fe")
		#expect(locallyDeleted == (cause == .deletedLocally))
	}
}
