@testable import ExampleSDK
import Testing
import XmtpSdk

@Test func unreadSDKSelection() {
	let options = unreadOptions(marker: 42, ownInboxID: "own")
	#expect(options.insertedAfter?.ns == 42)
	#expect(options.excludeSenderInboxIds == ["own"])
	#expect(options.deliveryStatus == .published)
	#expect(Set(options.contentTypes?.map(\.typeId) ?? []) == Set(["text", "markdown", "reply", "remoteStaticAttachment"]))
}
