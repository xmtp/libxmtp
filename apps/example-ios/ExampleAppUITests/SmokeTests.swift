import XCTest

@MainActor final class SmokeTests: XCTestCase {
	func testLaunchesToJoinServer() {
		let app = XCUIApplication()
		app.launchArguments = ["-reset-all-data"]
		app.launch()
		XCTAssertTrue(app.staticTexts["A place to connect."].waitForExistence(timeout: 20))
		XCTAssertTrue(app.buttons["join-server"].exists)
	}
}
