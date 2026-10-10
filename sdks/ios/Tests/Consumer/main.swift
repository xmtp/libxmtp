import Foundation
import XmtpSdk

// A bare executable has no bundle identifier, so default storage has no
// location: client creation must fail with `StorageLocationRequired`. An XCTest
// host always has a bundle identifier, so only this executable reaches that
// branch. `ClientOwnershipTests.testDefaultStorageUsesApplicationSupport`
// checks the branch with a bundle identifier.
func fail(_ message: String) -> Never {
	FileHandle.standardError.write(Data((message + "\n").utf8))
	exit(1)
}

guard Bundle.main.bundleIdentifier == nil else {
	fail("The bare executable has a bundle identifier")
}

do {
	_ = try await SDKClient.create(
		signer: generateLocalSigner(),
		options: ClientOptions(storage: StorageOptions(location: .default)),
	)
	fail("Default storage accepted a missing bundle identifier")
} catch XmtpError.StorageLocationRequired {
	// Expected: default storage has no location here.
} catch {
	fail("Unexpected error: \(error)")
}

print("Swift missing bundle identifier rejected")

/// Compile the SDK page records and omitted/default argument forms.
func consumeHistoryPageForms(_ group: Group, _ dm: Dm) async throws {
	let page: MessageHistoryPage = try await group.messageHistoryPage()
	_ = try await dm.messageHistoryPage()
	let position: MessageHistoryPosition? = page.lastPosition
	_ = try await group.messageHistoryPage(options: ListMessagesOptions(limit: 50), before: position)
	_ = try await dm.messageHistoryPage(after: position)
	let timestamp: Timestamp? = position?.sentAt
	let cursor: String? = position?.deliveryCursor
	precondition(timestamp == nil || cursor != nil)
}

func consumeRecoveryPageForms(_ group: Group, _ dm: Dm) async throws {
	let page: MessageRecoveryPage = try await group.messageRecoveryPage()
	_ = try await dm.messageRecoveryPage()
	let position: MessageRecoveryPosition? = page.lastPosition
	_ = try await group.messageRecoveryPage(options: ListMessagesOptions(limit: 50), before: position)
	_ = try await dm.messageRecoveryPage(after: position)
}
