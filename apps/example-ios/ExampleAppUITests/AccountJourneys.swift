import XCTest

@MainActor final class AccountJourneys: XCTestCase {
	override func setUpWithError() throws {
		try super.setUpWithError()
		continueAfterFailure = false
	}

	private func launch(reset: Bool = true) -> XCUIApplication {
		let app = XCUIApplication()
		if reset {
			app.launchArguments = ["-reset-all-data"]
		}
		let backendURL = ProcessInfo.processInfo.environment["XMTP_BACKEND_URL"] ?? "http://127.0.0.1:5050"
		app.launchEnvironment["XMTP_BACKEND_URL"] = backendURL
		app.launch()
		return app
	}

	private func join(_ app: XCUIApplication, claim: Bool, checkDefault: Bool = false) {
		let join = app.buttons["Join a server"]
		XCTAssertTrue(join.waitForExistence(timeout: 30))
		if checkDefault {
			let field = app.textFields["server-address"]
			let expected = ProcessInfo.processInfo.environment["XMTP_BACKEND_URL"] ?? "http://127.0.0.1:5050"
			XCTAssertEqual(field.value as? String, expected)
			field.tap()
			// A long field can place the caret mid-text. Repeat until its text is empty.
			for _ in 0 ..< 8 {
				let current = field.value as? String ?? ""
				if current.isEmpty || current == field.placeholderValue {
					break
				}
				field.tap()
				field.typeText(String(repeating: XCUIKeyboardKey.delete.rawValue, count: current.count))
			}
			XCTAssertTrue((field.value as? String)?.isEmpty == true || field.value as? String == field.placeholderValue)
			XCTAssertFalse(join.isEnabled)
			field.typeText(expected)
			XCTAssertEqual(field.value as? String, expected)
		}
		let deadline = Date().addingTimeInterval(30)
		while !join.isEnabled, Date() < deadline {
			RunLoop.current.run(until: Date().addingTimeInterval(0.1))
		}
		XCTAssertTrue(join.isEnabled); join.tap()
		let skip = app.buttons["Skip for now"]
		if skip.waitForExistence(timeout: 30) {
			if claim {
				let name = "ios" + UUID().uuidString.replacingOccurrences(of: "-", with: "").prefix(12).lowercased()
				let field = app.textFields["nick"]; XCTAssertTrue(field.waitForExistence(timeout: 10)); field.tap(); field
					.typeText(name)
				app.buttons["Continue"].tap()
				let button = app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "Claim ")).firstMatch
				XCTAssertTrue(button.waitForExistence(timeout: 30)); button.tap()
			} else {
				skip.tap()
			}
		}
		XCTAssertTrue(app.navigationBars["Chats"].waitForExistence(timeout: 30))
	}

	private func inbox(_ app: XCUIApplication) -> String {
		app.buttons["Settings"].tap()
		let value = app.descendants(matching: .any).matching(identifier: "profile-inbox-id").firstMatch
		XCTAssertTrue(value.waitForExistence(timeout: 10))
		let inbox = value.value as? String
		XCTAssertNotNil(inbox, "Inbox ID must expose its raw value through accessibilityValue")
		return inbox ?? ""
	}

	private func revealButton(_ title: String, in app: XCUIApplication, file: StaticString = #filePath,
	                          line: UInt = #line) -> XCUIElement
	{
		let button = app.buttons[title]
		let scroll: XCUIElement = app.collectionViews.firstMatch.exists ? app.collectionViews.firstMatch :
			(app.scrollViews.firstMatch.exists ? app.scrollViews.firstMatch : app)
		for _ in 0 ..< 10 {
			if button.exists, button.isHittable, app.frame.insetBy(dx: 0, dy: 44).contains(button.frame) {
				return button
			}
			scroll.swipeUp()
		}
		XCTAssertTrue(
			button.exists && button.isHittable && app.frame.insetBy(dx: 0, dy: 44).contains(button.frame),
			"Could not fully reveal Settings action: " + title,
			file: file,
			line: line,
		)
		return button
	}

	private func confirm(_ title: String, button: String, in app: XCUIApplication) {
		let alert = app.alerts[title]
		XCTAssertTrue(alert.waitForExistence(timeout: 10), "Expected confirmation: " + title)
		let action = alert.buttons[button]
		XCTAssertTrue(action.waitForExistence(timeout: 5))
		action.tap()
	}

	func testJoinRestoreAndSignOutKeepsAccount() {
		let app = launch(); join(app, claim: true, checkDefault: true)
		let original = inbox(app)
		// Kill the app soon after it leaves the foreground, as a swipe in the app switcher does. A lease left by the
		// killed process would still have most of its 30 seconds at relaunch.
		XCUIDevice.shared.press(.home)
		RunLoop.current.run(until: Date().addingTimeInterval(1))
		app.terminate()
		// The reset hook applies only to the first launch of this journey.
		app.launchArguments.removeAll { $0 == "-reset-all-data" }
		app.launch()
		XCTAssertTrue(app.navigationBars["Chats"].waitForExistence(timeout: 30))
		// A consumer lease left by the killed process fails the reader until it expires.
		let disconnected = app.staticTexts["Disconnected. Retrying…"]
		let deadline = Date().addingTimeInterval(10)
		while Date() < deadline {
			XCTAssertFalse(disconnected.exists, "The message reader failed after relaunch")
			RunLoop.current.run(until: Date().addingTimeInterval(0.25))
		}
		XCTAssertEqual(inbox(app), original)
		revealButton("Sign out", in: app).tap()
		confirm("Sign out?", button: "Sign out", in: app)
		join(app, claim: false)
		XCTAssertEqual(inbox(app), original)
	}

	func testDeleteLocalDataCreatesNewAccount() {
		let app = launch(); join(app, claim: false)
		let original = inbox(app)
		revealButton("Delete local data", in: app).tap()
		confirm("Delete local data?", button: "Delete local data", in: app)
		join(app, claim: false)
		XCTAssertNotEqual(inbox(app), original)
	}
}
