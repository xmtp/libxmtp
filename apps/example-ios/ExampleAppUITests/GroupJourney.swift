import ExampleCore
import Foundation
import UIKit
import XCTest
import XmtpSdk

/// J4 uses real members and verifies the received file's bytes through the SDK.
@MainActor final class GroupJourney: XCTestCase {
	private enum JourneyError: Error {
		case timedOut
	}

	private var backend: String {
		ProcessInfo.processInfo.environment["XMTP_BACKEND_URL"] ?? ProcessInfo.processInfo
			.environment["TEST_RUNNER_XMTP_BACKEND_URL"] ?? "http://127.0.0.1:5050"
	}

	private func withPeers(_ body: ([SDKClient]) async throws -> Void) async throws {
		let target = try BackendURL(backend)
		var clients: [SDKClient] = []
		for _ in 0 ..< 3 {
			try await clients.append(peer(target: target))
		}
		try await body(clients)
	}

	private func peer(target: BackendURL) async throws -> SDKClient {
		let root = FileManager.default.temporaryDirectory.appendingPathComponent("group-peer-" + UUID().uuidString)
		try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
		do {
			let client = try await SDKClient.create(
				signer: generateLocalSigner(),
				options: ClientOptions(
					backend: .options(options: BackendOptions(url: target.value)),
					storage: StorageOptions(location: .explicit(
						dbPath: root.appendingPathComponent("messages.db3").path,
						attachmentsDir: root.appendingPathComponent("attachments", isDirectory: true).path,
					)),
					deviceSync: false,
					attachments: AttachmentOptions(allowPrivateNetwork: target.allowPrivateNetwork),
				),
			)
			addTeardownBlock {
				try await client.end()
				try FileManager.default.removeItem(at: root)
			}
			return client
		} catch {
			let cause = error
			do { try FileManager.default.removeItem(at: root) } catch {}
			throw cause
		}
	}

	private func poll(_ condition: () async throws -> Bool) async throws {
		for _ in 0 ..< 120 {
			if try await condition() {
				return
			}
			try await Task.sleep(for: .milliseconds(250))
		}
		XCTFail("The expected group state did not arrive")
		throw JourneyError.timedOut
	}

	private func launch(file: Data, filename: String = "group-proof.txt") -> XCUIApplication {
		let app = XCUIApplication()
		app.launchArguments = ["-reset-all-data"]
		app.launchEnvironment["XMTP_BACKEND_URL"] = backend
		app.launchEnvironment["XMTP_ALLOW_PRIVATE_ATTACHMENTS"] = "1"
		app.launchEnvironment["XMTP_UI_TEST_ATTACHMENT"] = filename + ":" + file.base64EncodedString()
		app.launch()
		let join = app.buttons["Join a server"]
		XCTAssertTrue(join.waitForExistence(timeout: 30))
		let deadline = Date().addingTimeInterval(30)
		while !join.isEnabled, Date() < deadline {
			RunLoop.current.run(until: Date().addingTimeInterval(0.1))
		}
		XCTAssertTrue(join.isEnabled)
		join.tap()
		let skip = app.buttons["Skip for now"]
		if skip.waitForExistence(timeout: 30) {
			skip.tap()
		}
		XCTAssertTrue(app.navigationBars["Chats"].waitForExistence(timeout: 30))
		return app
	}

	private func button(_ app: XCUIApplication, containing text: String) -> XCUIElement {
		app.buttons.matching(NSPredicate(
			format: "label CONTAINS %@",
			text,
		)).firstMatch
	}

	private func group(_ client: SDKClient, named name: String) async throws -> XmtpSdk.Group? {
		_ = try await client.conversations.syncAll(consentStates: nil)
		for conversation in try await client.conversations
			.list()
		{
			if case let .group(group) = conversation, try await group.state().name == name {
				return group
			}
		}
		return nil
	}

	func testGroupMembersPermissionsAndVerifiedFile() async throws {
		continueAfterFailure = false
		try await withPeers { peers in
			let first = peers[0], second = peers[1], third = peers[2]
			let suffix = UUID().uuidString.replacingOccurrences(of: "-", with: "").prefix(10).lowercased()
			let firstHandle = "group" + suffix
			let thirdHandle = "added" + suffix
			try await first.claimHandle(handle: firstHandle)
			try await third.claimHandle(handle: thirdHandle)
			let bytes = Data("A group attachment verified by the receiving member.\n".utf8)
			let name = "Group proof " + suffix
			let app = launch(file: bytes)
			try await createGroup(app, name: name, handle: firstHandle, firstInboxID: first.inboxId(), second: second)
			try await manageMembers(app, name: name, first: first, third: third, handle: thirdHandle)
			try await verifyFile(app, name: name, first: first, bytes: bytes)
			try await removeMember(app, name: name, observer: first, member: third)
		}
	}

	private func createGroup(
		_ app: XCUIApplication,
		name: String,
		handle firstHandle: String,
		firstInboxID: String,
		second: SDKClient,
	) async throws {
		app.buttons["New message"].tap()
		app.buttons["Group"].tap()
		let input = app.textFields["Handle, Ethereum address, or inbox ID"]
		XCTAssertTrue(input.waitForExistence(timeout: 10))
		input.tap()
		input.typeText("@" + firstHandle + "," + second.inboxId())
		app.buttons["Add"].tap()
		let firstToken = app.descendants(matching: .any)["recipient." + firstInboxID]
		let secondToken = app.descendants(matching: .any)["recipient." + second.inboxId()]
		XCTAssertTrue(firstToken.waitForExistence(timeout: 20))
		XCTAssertTrue(secondToken.waitForExistence(timeout: 20))
		XCTAssertEqual(firstToken.label, "@" + firstHandle)
		XCTAssertEqual(secondToken.label, String(second.inboxId().prefix(12)))
		app.buttons["newConversation.next"].tap()
		let nameField = app.textFields["Group name"]
		XCTAssertTrue(nameField.waitForExistence(timeout: 10))
		nameField.tap()
		nameField.typeText(name)
		app.buttons["newConversation.create"].tap()
		let detailsButton = app.buttons["chat.details"]
		XCTAssertTrue(detailsButton.waitForExistence(timeout: 20))
		try await poll { detailsButton.label.contains("3 members") }
		detailsButton.tap()
		XCTAssertTrue(app.navigationBars["Details"].waitForExistence(timeout: 10))
		let firstRow = app.buttons["details.member." + firstInboxID]
		reveal(firstRow, in: app)
		let secondRow = app.buttons["details.member." + second.inboxId()]
		reveal(secondRow, in: app)
	}

	private func manageMembers(
		_ app: XCUIApplication,
		name: String,
		first: SDKClient,
		third: SDKClient,
		handle thirdHandle: String,
	) async throws {
		let addMember = app.buttons["details.add-member-shortcut"]
		reveal(addMember, in: app, missingIsAbove: true)
		addMember.tap()
		XCTAssertTrue(app.navigationBars["Add member"].waitForExistence(timeout: 10))
		let memberInput = app.descendants(matching: .any)["member.recipient"]
		XCTAssertTrue(memberInput.waitForExistence(timeout: 10))
		memberInput.tap()
		memberInput.typeText("@" + thirdHandle)
		XCTAssertTrue(app.staticTexts[thirdHandle].waitForExistence(timeout: 20))
		let add = app.navigationBars["Add member"].buttons["Add"]
		XCTAssertTrue(add.isEnabled)
		add.tap()
		try await poll { guard let joined = try await group(third, named: name) else { return false }
			return try await joined.members().contains { $0.inboxId == third.inboxId() }
		}
		let memberRow = app.buttons["details.member." + third.inboxId()]
		reveal(memberRow, in: app)
		XCTAssertTrue(memberRow.waitForExistence(timeout: 20))
		try await poll { !app.staticTexts["Pending"].exists }
		let permissions = button(app, containing: "Permissions")
		reveal(permissions, in: app)
		permissions.tap()
		XCTAssertTrue(app.navigationBars["Permissions"].waitForExistence(timeout: 10))
		button(app, containing: "Only admins can update group details and add people.").tap()
		try await poll { guard let joined = try await group(first, named: name) else { return false }
			return try await joined.state().permissions.policyType == .adminOnly
		}
		app.navigationBars.buttons.firstMatch.tap()
		reveal(memberRow, in: app)
		memberRow.tap()
		XCTAssertTrue(app.navigationBars["Member"].waitForExistence(timeout: 10))
		let makeAdmin = app.buttons["Make admin"]
		reveal(makeAdmin, in: app)
		makeAdmin.tap()
		try await poll { guard let joined = try await group(third, named: name) else { return false }
			return try await joined.state().admins.contains(third.inboxId())
		}
		let adminRole = app.staticTexts["Admin"]
		reveal(adminRole, in: app, missingIsAbove: true)
		XCTAssertTrue(adminRole.waitForExistence(timeout: 20))
		app.buttons["member.close"].tap()
		app.navigationBars.buttons.firstMatch.tap()
	}

	private func verifyFile(_ app: XCUIApplication, name: String, first: SDKClient, bytes: Data) async throws {
		let attachment = app.buttons["Add attachment"]
		XCTAssertTrue(attachment.waitForExistence(timeout: 10))
		attachment.tap()
		app.buttons["Test file"].tap()
		let send = app.buttons["Send"].firstMatch
		XCTAssertTrue(send.waitForExistence(timeout: 20))
		send.tap()
		var received: RemoteAttachment?
		try await poll {
			guard let joined = try await group(first, named: name) else { return false }
			try await joined.sync()
			for message in try await joined
				.messages(options: nil)
			{
				if case let .standard(.remoteAttachment(remote)) = message.content {
					received = remote
					return true
				}
			}
			return false
		}
		let remote = try XCTUnwrap(received)
		let downloaded = try await first.attachments().download(remote: remote)
		XCTAssertEqual(try Data(contentsOf: URL(fileURLWithPath: downloaded.path)), bytes)
		let detailsButton = app.buttons["chat.details"]
		XCTAssertTrue(detailsButton.waitForExistence(timeout: 20))
		try await poll { detailsButton.label.contains("4 members") }
		detailsButton.tap()
		let media = button(app, containing: "Media and files")
		reveal(media, in: app)
		media.tap()
		XCTAssertTrue(app.navigationBars["Media and files"].waitForExistence(timeout: 10))
		XCTAssertTrue(app.staticTexts["group-proof.txt"].waitForExistence(timeout: 20))
	}
}

extension GroupJourney {
	func testOwnImageRestoresAndShowsDetails() async throws {
		continueAfterFailure = false
		let renderer = UIGraphicsImageRenderer(size: CGSize(width: 320, height: 180), format: {
			let format = UIGraphicsImageRendererFormat(); format.scale = 1; return format
		}())
		let bytes = renderer.pngData { context in
			UIColor.systemTeal.setFill(); context.fill(CGRect(x: 0, y: 0, width: 320, height: 180))
			UIColor.systemYellow.setFill(); context.cgContext.fillEllipse(in: CGRect(x: 100, y: 30, width: 120, height: 120))
		}
		try await withPeers { peers in
			let handle = "image" + UUID().uuidString.replacingOccurrences(of: "-", with: "").prefix(10).lowercased()
			try await peers[0].claimHandle(handle: handle)
			let app = launch(file: bytes, filename: "image-proof.png")
			try await createGroup(app, name: "Image proof", handle: handle, firstInboxID: peers[0].inboxId(), second: peers[1])
			app.navigationBars.buttons.firstMatch.tap()
			app.buttons["Add attachment"].tap(); app.buttons["Test file"].tap()
			let send = app.buttons["Send"].firstMatch
			XCTAssertTrue(send.waitForExistence(timeout: 20)); send.tap()
			let image = app.descendants(matching: .any).matching(NSPredicate(format: "label == %@", "Image attachment"))
				.firstMatch
			XCTAssertTrue(image.waitForExistence(timeout: 20))
			try await poll { (image.value as? String)?.contains("Verified") == true }
			try await poll { AttachmentPixelProbe.matches(image, in: app) }
			func capture(_ name: String) {
				let attachment = XCTAttachment(screenshot: app.screenshot()); attachment.name = name
				attachment.lifetime = .keepAlways; self.add(attachment)
			}
			capture("Own image loaded automatically")
			app.navigationBars.buttons.firstMatch.tap()
			let chat = app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "Image proof")).firstMatch
			XCTAssertTrue(chat.waitForExistence(timeout: 10)); chat.tap()
			XCTAssertTrue(image.waitForExistence(timeout: 20))
			try await poll { (image.value as? String)?.contains("Verified") == true }
			try await poll { AttachmentPixelProbe.matches(image, in: app) }
			capture("Own image restored on return")
			image.tap()
			XCTAssertTrue((image.value as? String)?.contains("You") == true)
			XCTAssertFalse(app.staticTexts["image-proof.png"].exists)
			capture("Image sender and time overlay")
			image.press(forDuration: 1)
			let details = app.buttons["Details"]; XCTAssertTrue(details.waitForExistence(timeout: 10))
			capture("Image actions and quick reactions"); details.tap()
			XCTAssertTrue(app.staticTexts["Filename, image-proof.png"].waitForExistence(timeout: 10))
			XCTAssertTrue(app.staticTexts["Dimensions, 320 × 180"].waitForExistence(timeout: 10))
			capture("Image file details")
		}
	}
}

private extension GroupJourney {
	func removeMember(_ app: XCUIApplication, name: String, observer: SDKClient, member: SDKClient) async throws {
		app.navigationBars.buttons.firstMatch.tap()
		let row = app.buttons["details.member." + member.inboxId()]
		reveal(row, in: app); row.tap()
		XCTAssertTrue(app.navigationBars["Member"].waitForExistence(timeout: 10))
		let remove = app.buttons["member.remove"]
		reveal(remove, in: app)
		waitUntilHittable(remove)
		remove.tap()
		let confirm = app.sheets.buttons["member.confirm-remove"].firstMatch
		waitUntilHittable(confirm)
		confirm.tap()
		try await poll {
			guard let joined = try await group(observer, named: name)
			else { return false }; return try await !joined.members().contains { $0.inboxId == member.inboxId() }
		}
		XCTAssertEqual(
			XCTWaiter.wait(for: [XCTNSPredicateExpectation(
				predicate: NSPredicate(format: "exists == false"),
				object: app.navigationBars["Member"],
			)], timeout: 20),
			.completed,
		)
		XCTAssertTrue(app.navigationBars["Details"].exists)
	}

	func waitUntilHittable(_ element: XCUIElement, file: StaticString = #filePath, line: UInt = #line) {
		let ready = XCTNSPredicateExpectation(
			predicate: NSPredicate(format: "exists == true AND hittable == true"),
			object: element,
		)
		XCTAssertEqual(
			XCTWaiter.wait(for: [ready], timeout: 10),
			.completed,
			"Action \(element.identifier) must be hittable",
			file: file,
			line: line,
		)
	}

	func fullyVisible(_ element: XCUIElement, in app: XCUIApplication) -> Bool {
		guard element.exists, element.isHittable else { return false }
		let bars = app.navigationBars
		let navigation = bars.element(boundBy: max(0, bars.count - 1))
		let top = max(app.frame.minY + 44, navigation.exists ? navigation.frame.maxY + 8 : app.frame.minY + 44)
		return element.frame.minY >= top && element.frame.maxY <= app.frame.maxY - 44
	}

	func reveal(
		_ element: XCUIElement,
		in app: XCUIApplication,
		missingIsAbove: Bool = false,
		file: StaticString = #filePath,
		line: UInt = #line,
	) {
		let collections = app.collectionViews
		let scroll = collections.count > 0 ? collections.element(boundBy: collections.count - 1) : app
		for step in 0 ..< 48 {
			if fullyVisible(element, in: app) {
				return
			}
			let navigation = app.navigationBars.firstMatch
			let top = max(app.frame.minY + 44, navigation.exists ? navigation.frame.maxY + 8 : app.frame.minY + 44)
			let moveDown = (!element.exists && (missingIsAbove || step >= 16))
				|| (element.exists && element.frame.height > 0 && element.frame.minY < top)
			let from = scroll.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: moveDown ? 0.4 : 0.6))
			let destination = scroll.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: moveDown ? 0.6 : 0.4))
			from.press(forDuration: 0.05, thenDragTo: destination)
		}
		let target = element.exists ? "\(element.identifier) [\(element.label)] frame=\(element.frame)" : "missing target"
		let description = "Action \(target), "
			+ "navigation=\(app.navigationBars.firstMatch.frame), app=\(app.frame) must be fully visible"
		XCTContext.runActivity(named: description) { activity in
			activity.add(XCTAttachment(screenshot: app.screenshot()))
			activity.add(XCTAttachment(string: app.debugDescription))
		}
		XCTAssertTrue(fullyVisible(element, in: app), description, file: file, line: line)
	}
}
