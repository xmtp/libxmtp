import XCTest

@MainActor final class SmokeTests: XCTestCase {
	func testLaunchesToJoinServer() {
		let app = XCUIApplication()
		app.launchArguments = ["-reset-all-data"]
		app.launch()
		XCTAssertTrue(app.staticTexts["Join a server"].waitForExistence(timeout: 20))
	}
}
