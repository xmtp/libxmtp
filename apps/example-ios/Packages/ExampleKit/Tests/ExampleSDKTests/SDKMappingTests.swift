import ExampleCore
@testable import ExampleSDK
import Foundation
import Testing

struct SDKMappingTests {
	@Test(arguments: [false, true])
	func attachments(optIn: Bool) throws {
		for url in ["http://127.0.0.1:5050", "https://example.com"] {
			let options = try sessionOptions(
				profile: BackendProfile(url: url, inboxID: "saved"),
				paths: ProfilePaths(root: URL(fileURLWithPath: "/tmp/profile")),
				databaseKey: Data(repeating: 1, count: 32),
				allowPrivateAttachments: optIn,
			)
			#expect(options.attachments?.allowPrivateNetwork == (optIn && url.contains("127.0.0.1")))
			#expect(options.allowOffline)
		}
	}
}
