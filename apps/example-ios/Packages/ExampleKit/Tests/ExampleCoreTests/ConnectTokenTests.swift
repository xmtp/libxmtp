@testable import ExampleCore
import Testing

struct ConnectTokenTests {
	@Test(arguments: ["", " \n\t"])
	func requiredBlankTokenCannotJoin(token: String) {
		var state = ConnectState(address: "https://example.com")
		state.requiresToken = true; state.token = token
		#expect(!state.canJoin)
		state.requiresToken = false
		#expect(state.canJoin)
	}
}
