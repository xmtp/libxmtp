@testable import ExampleCore
import Testing

struct ConnectTokenTests {
	@Test(arguments: ["", " \n\t"])
	func requiredBlankTokenCannotJoin(token: String) {
		var state = ConnectState(address: "https://example.com")
		state.ready = true; state.requiresToken = true; state.token = token
		#expect(!state.canJoin)
		state.requiresToken = false
		#expect(state.canJoin)
	}

	@Test(arguments: ["", " ", " \n\t"])
	func blankAddressCannotJoin(address: String) {
		var state = ConnectState(address: address)
		state.ready = true
		#expect(!state.canJoin)
		state.address = "https://example.com"
		#expect(state.canJoin)
	}
}
