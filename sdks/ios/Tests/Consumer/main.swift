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
} catch XmtpError.StorageLocationRequired {}
print("Swift missing bundle identifier rejected")
