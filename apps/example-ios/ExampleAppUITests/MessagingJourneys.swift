import Foundation
import XCTest
import XmtpSdk

@MainActor final class MessagingJourneys: XCTestCase {
	private var backend: String {
		ProcessInfo.processInfo.environment["XMTP_BACKEND_URL"] ?? "http://127.0.0.1:5050"
	}

	private func identified(_ id: String, in app: XCUIApplication) -> XCUIElement {
		app.descendants(matching: .any).matching(identifier: id).firstMatch
	}

	@discardableResult private func wait(_ element: XCUIElement, timeout: TimeInterval = 10,
	                                     file: StaticString = #filePath, line: UInt = #line) -> XCUIElement
	{
		let exists = element.waitForExistence(timeout: timeout)
		if !exists {
			let app = XCUIApplication()
			let screenshot = XCTAttachment(screenshot: app.screenshot())
			screenshot.lifetime = .keepAlways
			add(screenshot)
			let hierarchy = XCTAttachment(string: app.debugDescription)
			hierarchy.lifetime = .keepAlways
			add(hierarchy)
		}
		XCTAssertTrue(exists, "Missing element: \(element)", file: file, line: line)
		return element
	}

	private func waitUntil(_ predicate: String, element: XCUIElement,
	                       file: StaticString = #filePath, line: UInt = #line)
	{
		let expectation = XCTNSPredicateExpectation(predicate: NSPredicate(format: predicate), object: element)
		XCTAssertEqual(XCTWaiter.wait(for: [expectation], timeout: 15), .completed,
		               "Condition \(predicate) failed for \(element)", file: file, line: line)
	}

	/// Joins a fresh account and claims a new handle for it during onboarding.
	private func launch() -> XCUIApplication {
		continueAfterFailure = false
		let app = XCUIApplication()
		app.launchArguments = ["-reset-all-data"]
		app.launchEnvironment["XMTP_BACKEND_URL"] = backend
		app.launch()
		let join = wait(app.buttons["Join a server"], timeout: 30)
		waitUntil("enabled == true", element: join)
		join.tap()
		wait(app.buttons["Skip for now"], timeout: 30)
		let name = "ios" + UUID().uuidString.replacingOccurrences(of: "-", with: "").prefix(12).lowercased()
		let field = wait(app.textFields["nick"])
		field.tap()
		acknowledgeKeyboardIntro(app)
		field.typeText(name)
		app.buttons["Continue"].tap()
		wait(app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "Claim ")).firstMatch, timeout: 30).tap()
		wait(app.navigationBars["Chats"], timeout: 30)
		return app
	}

	private func peer() async throws -> SDKClient {
		let client = try await SDKClient.create(signer: generateLocalSigner(), options: ClientOptions(
			backend: .options(options: BackendOptions(url: backend)),
			storage: StorageOptions(location: .inMemory), deviceSync: false,
		))
		addTeardownBlock { try await client.end() }
		return client
	}

	private func inbox(_ app: XCUIApplication) -> String {
		app.buttons["Settings"].tap()
		let value = wait(identified("profile-inbox-id", in: app))
		guard let inbox = value.value as? String else { XCTFail("Inbox ID has no accessibility value"); return "" }
		app.navigationBars.buttons.firstMatch.tap()
		return inbox
	}

	private func acknowledgeKeyboardIntro(_ app: XCUIApplication) {
		let panel = app.otherElements["UIContinuousPathIntroductionView"]
		guard panel.waitForExistence(timeout: 2) else { return }
		let instruction = panel.staticTexts[
			"Speed up your typing by sliding your finger across the letters to compose a word.",
		]
		guard instruction.exists else { XCTFail("Unexpected keyboard introduction content"); return }
		let advance = wait(panel.buttons["Continue"])
		waitUntil("hittable == true", element: advance)
		advance.tap()
		waitUntil("exists == false", element: panel)
	}

	private func composer(_ app: XCUIApplication) -> XCUIElement {
		wait(identified("message-composer", in: app))
	}

	private func send(_ text: String, app: XCUIApplication) {
		let field = composer(app)
		waitUntil("enabled == true", element: field)
		field.tap()
		acknowledgeKeyboardIntro(app)
		field.typeText(text)
		let send = wait(identified("send-message", in: app))
		waitUntil("enabled == true", element: send)
		send.tap()
	}

	private func poll(_ condition: () async throws -> Bool) async throws {
		for _ in 0 ..< 60 {
			if try await condition() {
				return
			}
			try await Task.sleep(for: .milliseconds(250))
		}
		XCTFail("The expected SDK state did not arrive")
	}

	private func startByHandle(_ handle: String, inbox: String, app: XCUIApplication) {
		app.buttons["New message"].tap()
		let input = wait(app.textFields["Handle, Ethereum address, or inbox ID"])
		input.tap()
		acknowledgeKeyboardIntro(app)
		input.typeText("@" + handle.uppercased())
		app.buttons["Add"].tap()
		let token = wait(identified("recipient." + inbox, in: app), timeout: 20)
		XCTAssertTrue(token.label.localizedCaseInsensitiveContains(handle))
		let next = wait(identified("newConversation.next", in: app))
		waitUntil("enabled == true", element: next)
		next.tap()
		wait(identified("newConversation.create", in: app)).tap()
		let header = wait(identified("chat.details", in: app), timeout: 20)
		XCTAssertTrue(header.label.localizedCaseInsensitiveContains(handle))
	}

	private func selectThumbsUp(on message: XCUIElement, app: XCUIApplication) {
		message.press(forDuration: 1)
		wait(app.buttons["React"]).tap()
		let search = wait(identified("emoji.search", in: app))
		search.tap()
		acknowledgeKeyboardIntro(app)
		search.typeText("thumbs up")
		let option = wait(identified("emoji.option.👍", in: app))
		waitUntil("hittable == true", element: option)
		option.tap()
		waitUntil("exists == false", element: identified("emoji.close", in: app))
	}

	private func exerciseMessageActions(_ app: XCUIApplication) {
		let reply = wait(app.staticTexts["peer reply"].firstMatch, timeout: 20)
		acknowledgeKeyboardIntro(app)
		reply.press(forDuration: 1)
		XCTAssertFalse(app.buttons["Delete"].exists)
		let replyAction = wait(app.buttons["Reply"])
		waitUntil("hittable == true", element: replyAction)
		replyAction.tap()
		send("quoted reply", app: app)
		let own = wait(app.staticTexts["quoted reply"].firstMatch)
		selectThumbsUp(on: own, app: app)
		let badge = app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "👍")).firstMatch
		wait(badge)
		selectThumbsUp(on: own, app: app)
		waitUntil("exists == false", element: badge)
		own.press(forDuration: 1)
		wait(app.buttons["Delete"]).tap()
		wait(app.alerts.buttons["Delete message"]).tap()
		wait(app.staticTexts["Message deleted"].firstMatch, timeout: 20)
	}

	private func verifyDraftRelaunch(_ app: XCUIApplication, handle: String, inbox original: String) {
		let field = composer(app)
		field.tap()
		acknowledgeKeyboardIntro(app)
		field.typeText("saved draft")
		// Kill the app soon after it leaves the foreground, as a swipe in the app switcher does. A lease left by the
		// killed process would still have most of its 30 seconds at relaunch.
		XCUIDevice.shared.press(.home)
		RunLoop.current.run(until: Date().addingTimeInterval(1))
		app.terminate()
		app.launchArguments = []
		app.launch()
		wait(app.navigationBars["Chats"], timeout: 30)
		// A consumer lease left by the killed process fails the reader until it expires.
		let disconnected = app.staticTexts["Disconnected. Retrying…"]
		let deadline = Date().addingTimeInterval(10)
		while Date() < deadline {
			XCTAssertFalse(disconnected.exists, "The message reader failed after relaunch")
			RunLoop.current.run(until: Date().addingTimeInterval(0.25))
		}
		XCTAssertEqual(inbox(app), original)
		let row = app.buttons.matching(NSPredicate(format: "label CONTAINS %@", handle)).firstMatch
		for _ in 0 ..< 8 {
			if row.exists, row.isHittable {
				break
			}
			app.collectionViews.firstMatch.swipeUp()
		}
		wait(row).tap()
		XCTAssertEqual(composer(app).value as? String, "saved draft")
	}

	private func openRequests(_ app: XCUIApplication) {
		wait(app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "Message requests")).firstMatch).tap()
	}

	private func requestRow(_ inbox: String, app: XCUIApplication) -> XCUIElement {
		wait(app.buttons.matching(NSPredicate(format: "label CONTAINS %@", String(inbox.prefix(12)))).firstMatch, timeout: 30)
	}

	/// An incoming DM waits in Message requests with no composer until Accept.
	private func acceptRequest(from requester: SDKClient, to recipient: String, app: XCUIApplication) async throws {
		let incoming = try await requester.conversations.createDm(peer: recipient)
		_ = try await incoming.sendText(text: "request one")
		openRequests(app)
		requestRow(requester.inboxId(), app: app).tap()
		wait(app.buttons["Accept"])
		XCTAssertFalse(identified("message-composer", in: app).exists)
		app.buttons["Accept"].tap()
		send("accepted reply", app: app)
		wait(app.navigationBars.buttons["Chats"]).tap()
		wait(app.navigationBars["Chats"])
	}

	/// Claims a handle at join, accepts a message request, starts a DM by handle, uses message actions, and keeps a
	/// draft and a working message reader across a background kill and relaunch.
	func testDirectMessageByHandleActionsAndDraft() async throws {
		continueAfterFailure = false
		let peer = try await peer()
		let requester = try await self.peer()
		do {
			let handle = "peer" + UUID().uuidString.replacingOccurrences(of: "-", with: "").prefix(10).lowercased()
			try await peer.claimHandle(handle: handle)
			let app = launch()
			let recipient = inbox(app)
			try await acceptRequest(from: requester, to: recipient, app: app)
			startByHandle(handle, inbox: peer.inboxId(), app: app)
			send("hello from iOS", app: app)
			try await poll {
				try await peer.conversations.sync()
				_ = try await peer.conversations.syncAll(consentStates: nil)
				guard let chat = try await peer.conversations.list().first else { return false }
				return try await chat.messages(options: nil).contains { message in
					if case .standard(.text("hello from iOS")) = message.content {
						return true
					}
					return false
				}
			}
			let conversations = try await peer.conversations.list()
			let chat = try XCTUnwrap(conversations.first)
			try await chat.updateConsentState(state: .allowed)
			_ = try await chat.sendText(text: "peer reply", options: nil)
			exerciseMessageActions(app)
			verifyDraftRelaunch(app, handle: handle, inbox: recipient)
			try await requester.end()
			try await peer.end()
		} catch { try await requester.end(); try await peer.end(); throw error }
	}
}
