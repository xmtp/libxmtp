//
//  GroupTests.swift
//
//
//  Created by Pat Nakajima on 2/1/24.
//

import CryptoKit
import XCTest
@testable import XMTPiOS
import LibXMTP
import XMTPTestHelpers

func assertThrowsAsyncError<T>(
		_ expression: @autoclosure () async throws -> T,
		_ message: @autoclosure () -> String = "",
		file: StaticString = #filePath,
		line: UInt = #line,
		_ errorHandler: (_ error: Error) -> Void = { _ in }
) async {
		do {
				_ = try await expression()
				// expected error to be thrown, but it was not
				let customMessage = message()
				if customMessage.isEmpty {
						XCTFail("Asynchronous call did not throw an error.", file: file, line: line)
				} else {
						XCTFail(customMessage, file: file, line: line)
				}
		} catch {
				errorHandler(error)
		}
}

@available(iOS 16, *)
class GroupTests: XCTestCase {
	// Use these fixtures to talk to the local node
	struct LocalFixtures {
		var alice: PrivateKey!
		var bob: PrivateKey!
		var fred: PrivateKey!
		var aliceClient: Client!
		var bobClient: Client!
		var fredClient: Client!
	}

	func localFixtures() async throws -> LocalFixtures {
		let alice = try PrivateKey.generate()
		let aliceClient = try await Client.create(
			account: alice,
			options: .init(
				api: .init(env: .local, isSecure: false),
				codecs: [GroupMembershipChangedCodec()],
				mlsAlpha: true
			)
		)
		let bob = try PrivateKey.generate()
		let bobClient = try await Client.create(
			account: bob,
			options: .init(
				api: .init(env: .local, isSecure: false),
				codecs: [GroupMembershipChangedCodec()],
				mlsAlpha: true
			)
		)
		let fred = try PrivateKey.generate()
		let fredClient = try await Client.create(
			account: fred,
			options: .init(
				api: .init(env: .local, isSecure: false),
				codecs: [GroupMembershipChangedCodec()],
				mlsAlpha: true
			)
		)

		return .init(
			alice: alice,
			bob: bob,
			fred: fred,
			aliceClient: aliceClient,
			bobClient: bobClient,
			fredClient: fredClient
		)
	}

	func testCanCreateAGroupWithDefaultPermissions() async throws {
		let fixtures = try await localFixtures()
		let bobGroup = try await fixtures.bobClient.conversations.newGroup(with: [fixtures.alice.address])
		try await fixtures.aliceClient.conversations.sync()
		let aliceGroup = try await fixtures.aliceClient.conversations.groups().first!
		XCTAssert(!bobGroup.id.isEmpty)
		XCTAssert(!aliceGroup.id.isEmpty)
		
		try await aliceGroup.addMembers(addresses: [fixtures.fred.address])
		try await bobGroup.sync()
		XCTAssertEqual(aliceGroup.memberAddresses.count, 3)
		XCTAssertEqual(bobGroup.memberAddresses.count, 3)

		try await aliceGroup.removeMembers(addresses: [fixtures.fred.address])
		try await bobGroup.sync()
		XCTAssertEqual(aliceGroup.memberAddresses.count, 2)
		XCTAssertEqual(bobGroup.memberAddresses.count, 2)

		try await bobGroup.addMembers(addresses: [fixtures.fred.address])
		try await aliceGroup.sync()
		XCTAssertEqual(aliceGroup.memberAddresses.count, 3)
		XCTAssertEqual(bobGroup.memberAddresses.count, 3)
		
		XCTAssertEqual(try bobGroup.permissionLevel(), .everyoneIsAdmin)
		XCTAssertEqual(try aliceGroup.permissionLevel(), .everyoneIsAdmin)
		XCTAssertEqual(try bobGroup.adminAddress().lowercased(), fixtures.bobClient.address.lowercased())
		XCTAssertEqual(try aliceGroup.adminAddress().lowercased(), fixtures.bobClient.address.lowercased())
		XCTAssert(try bobGroup.isAdmin())
		XCTAssert(try !aliceGroup.isAdmin())
	}

	func testCanCreateAGroupWithAdminPermissions() async throws {
		let fixtures = try await localFixtures()
		let bobGroup = try await fixtures.bobClient.conversations.newGroup(with: [fixtures.alice.address], permissions: GroupPermissions.groupCreatorIsAdmin)
		try await fixtures.aliceClient.conversations.sync()
		let aliceGroup = try await fixtures.aliceClient.conversations.groups().first!
		XCTAssert(!bobGroup.id.isEmpty)
		XCTAssert(!aliceGroup.id.isEmpty)

		let bobConsentResult = await fixtures.bobClient.contacts.consentList.groupState(groupId: bobGroup.id)
		XCTAssertEqual(bobConsentResult, ConsentState.allowed)

		let aliceConsentResult = await fixtures.aliceClient.contacts.consentList.groupState(groupId: aliceGroup.id)
		XCTAssertEqual(aliceConsentResult, ConsentState.unknown)

		try await bobGroup.addMembers(addresses: [fixtures.fred.address])
		try await aliceGroup.sync()
		XCTAssertEqual(aliceGroup.memberAddresses.count, 3)
		XCTAssertEqual(bobGroup.memberAddresses.count, 3)

		await assertThrowsAsyncError(
			try await aliceGroup.removeMembers(addresses: [fixtures.fred.address])
		)
		try await bobGroup.sync()
		XCTAssertEqual(aliceGroup.memberAddresses.count, 3)
		XCTAssertEqual(bobGroup.memberAddresses.count, 3)
		
		try await bobGroup.removeMembers(addresses: [fixtures.fred.address])
		try await aliceGroup.sync()
		XCTAssertEqual(aliceGroup.memberAddresses.count, 2)
		XCTAssertEqual(bobGroup.memberAddresses.count, 2)

		await assertThrowsAsyncError(
			try await aliceGroup.addMembers(addresses: [fixtures.fred.address])
		)
		try await bobGroup.sync()
		XCTAssertEqual(aliceGroup.memberAddresses.count, 2)
		XCTAssertEqual(bobGroup.memberAddresses.count, 2)
		
		XCTAssertEqual(try bobGroup.permissionLevel(), .groupCreatorIsAdmin)
		XCTAssertEqual(try aliceGroup.permissionLevel(), .groupCreatorIsAdmin)
		XCTAssertEqual(try bobGroup.adminAddress().lowercased(), fixtures.bobClient.address.lowercased())
		XCTAssertEqual(try aliceGroup.adminAddress().lowercased(), fixtures.bobClient.address.lowercased())
		XCTAssert(try bobGroup.isAdmin())
		XCTAssert(try !aliceGroup.isAdmin())
	}

	func testCanListGroups() async throws {
		let fixtures = try await localFixtures()
		_ = try await fixtures.aliceClient.conversations.newGroup(with: [fixtures.bob.address])

		let aliceGroupCount = try await fixtures.aliceClient.conversations.groups().count

		try await fixtures.bobClient.conversations.sync()
		let bobGroupCount = try await fixtures.bobClient.conversations.groups().count

		XCTAssertEqual(1, aliceGroupCount)
		XCTAssertEqual(1, bobGroupCount)
	}
	
	func testCanListGroupsAndConversations() async throws {
		let fixtures = try await localFixtures()
		_ = try await fixtures.aliceClient.conversations.newGroup(with: [fixtures.bob.address])
		_ = try await fixtures.aliceClient.conversations.newConversation(with: fixtures.bob.address)

		let aliceGroupCount = try await fixtures.aliceClient.conversations.list(includeGroups: true).count

		try await fixtures.bobClient.conversations.sync()
		let bobGroupCount = try await fixtures.bobClient.conversations.list(includeGroups: true).count

		XCTAssertEqual(2, aliceGroupCount)
		XCTAssertEqual(2, bobGroupCount)
	}

	func testCanListGroupMembers() async throws {
		let fixtures = try await localFixtures()
		let group = try await fixtures.aliceClient.conversations.newGroup(with: [fixtures.bob.address])

		try await group.sync()
		let members = group.memberAddresses.map(\.localizedLowercase).sorted()
		let peerMembers = Conversation.group(group).peerAddresses.map(\.localizedLowercase).sorted()

		XCTAssertEqual([fixtures.bob.address.localizedLowercase, fixtures.alice.address.localizedLowercase].sorted(), members)
		XCTAssertEqual([fixtures.bob.address.localizedLowercase].sorted(), peerMembers)
	}

	func testCanAddGroupMembers() async throws {
		let fixtures = try await localFixtures()
		let group = try await fixtures.aliceClient.conversations.newGroup(with: [fixtures.bob.address])

		try await group.addMembers(addresses: [fixtures.fred.address])

		try await group.sync()
		let members = group.memberAddresses.map(\.localizedLowercase).sorted()

		XCTAssertEqual([
			fixtures.bob.address.localizedLowercase,
			fixtures.alice.address.localizedLowercase,
			fixtures.fred.address.localizedLowercase
		].sorted(), members)

		let groupChangedMessage: GroupMembershipChanges = try await group.messages().first!.content()
		XCTAssertEqual(groupChangedMessage.membersAdded.map(\.accountAddress.localizedLowercase), [fixtures.fred.address.localizedLowercase])
	}

	func testCanRemoveMembers() async throws {
		let fixtures = try await localFixtures()
		let group = try await fixtures.aliceClient.conversations.newGroup(with: [fixtures.bob.address, fixtures.fred.address])

		try await group.sync()
		let members = group.memberAddresses.map(\.localizedLowercase).sorted()

		XCTAssertEqual([
			fixtures.bob.address.localizedLowercase,
			fixtures.alice.address.localizedLowercase,
			fixtures.fred.address.localizedLowercase
		].sorted(), members)

		try await group.removeMembers(addresses: [fixtures.fred.address])

		try await group.sync()

		let newMembers = group.memberAddresses.map(\.localizedLowercase).sorted()
		XCTAssertEqual([
			fixtures.bob.address.localizedLowercase,
			fixtures.alice.address.localizedLowercase,
		].sorted(), newMembers)

		let groupChangedMessage: GroupMembershipChanges = try await group.messages().first!.content()
		XCTAssertEqual(groupChangedMessage.membersRemoved.map(\.accountAddress.localizedLowercase), [fixtures.fred.address.localizedLowercase])
	}
	
	func testCanMessage() async throws {
		let fixtures = try await localFixtures()
		let notOnNetwork = try PrivateKey.generate()
		let canMessage = try await fixtures.aliceClient.canMessageV3(addresses: [fixtures.bobClient.address])
		let cannotMessage = try await fixtures.aliceClient.canMessageV3(addresses: [notOnNetwork.address, fixtures.bobClient.address])
		XCTAssert(canMessage)
		XCTAssert(!cannotMessage)
	}
	
	func testIsActive() async throws {
		let fixtures = try await localFixtures()
		let group = try await fixtures.aliceClient.conversations.newGroup(with: [fixtures.bob.address, fixtures.fred.address])

		try await group.sync()
		let members = group.memberAddresses.map(\.localizedLowercase).sorted()

		XCTAssertEqual([
			fixtures.bob.address.localizedLowercase,
			fixtures.alice.address.localizedLowercase,
			fixtures.fred.address.localizedLowercase
		].sorted(), members)
		
		try await fixtures.fredClient.conversations.sync()
		let fredGroup = try await fixtures.fredClient.conversations.groups().first
		try await fredGroup?.sync()

		var isAliceActive = try group.isActive()
		var isFredActive = try fredGroup!.isActive()
		
		XCTAssert(isAliceActive)
		XCTAssert(isFredActive)

		try await group.removeMembers(addresses: [fixtures.fred.address])

		try await group.sync()

		let newMembers = group.memberAddresses.map(\.localizedLowercase).sorted()
		XCTAssertEqual([
			fixtures.bob.address.localizedLowercase,
			fixtures.alice.address.localizedLowercase,
		].sorted(), newMembers)
		
		try await fredGroup?.sync()
		
		isAliceActive = try group.isActive()
		isFredActive = try fredGroup!.isActive()
		
		XCTAssert(isAliceActive)
		XCTAssert(!isFredActive)
	}


	func testCannotStartGroupWithSelf() async throws {
		let fixtures = try await localFixtures()

		await assertThrowsAsyncError(
			try await fixtures.aliceClient.conversations.newGroup(with: [fixtures.alice.address])
		)
	}

	func testCannotStartEmptyGroup() async throws {
		let fixtures = try await localFixtures()

		await assertThrowsAsyncError(
			try await fixtures.aliceClient.conversations.newGroup(with: [])
		)
	}

	func testCannotStartGroupWithNonRegisteredIdentity() async throws {
		let fixtures = try await localFixtures()

		let nonRegistered = try PrivateKey.generate()

		do {
			_ = try await fixtures.aliceClient.conversations.newGroup(with: [nonRegistered.address])

			XCTFail("did not throw error")
		} catch {
			if case let GroupError.memberNotRegistered(addresses) = error {
				XCTAssertEqual([nonRegistered.address.lowercased()], addresses.map { $0.lowercased() })
			} else {
				XCTFail("did not throw correct error")
			}
		}
	}

	func testGroupStartsWithAllowedState() async throws {
		let fixtures = try await localFixtures()
		let bobGroup = try await fixtures.bobClient.conversations.newGroup(with: [fixtures.alice.walletAddress])

		_ = try await bobGroup.send(content: "howdy")
		_ = try await bobGroup.send(content: "gm")
		try await bobGroup.sync()

		let isGroupAllowedResult = await fixtures.bobClient.contacts.isGroupAllowed(groupId: bobGroup.id)
		XCTAssertTrue(isGroupAllowedResult)

		let groupStateResult = await fixtures.bobClient.contacts.consentList.groupState(groupId: bobGroup.id)
		XCTAssertEqual(groupStateResult, ConsentState.allowed)
	}
	
	func testCanSendMessagesToGroup() async throws {
		let fixtures = try await localFixtures()
		let aliceGroup = try await fixtures.aliceClient.conversations.newGroup(with: [fixtures.bob.address])

		try await fixtures.bobClient.conversations.sync()
		let bobGroup = try await fixtures.bobClient.conversations.groups()[0]

		_ = try await aliceGroup.send(content: "sup gang original")
		_ = try await aliceGroup.send(content: "sup gang")

		try await aliceGroup.sync()
		let aliceGroupsCount = try await aliceGroup.messages().count
		XCTAssertEqual(3, aliceGroupsCount)
		let aliceMessage = try await aliceGroup.messages().first!

		try await bobGroup.sync()
		let bobGroupsCount = try await bobGroup.messages().count
		XCTAssertEqual(2, bobGroupsCount)
		let bobMessage = try await bobGroup.messages().first!

		XCTAssertEqual("sup gang", try aliceMessage.content())
		XCTAssertEqual("sup gang", try bobMessage.content())
	}
	
	func testCanSendMessagesToGroupDecrypted() async throws {
		let fixtures = try await localFixtures()
		let aliceGroup = try await fixtures.aliceClient.conversations.newGroup(with: [fixtures.bob.address])

		try await fixtures.bobClient.conversations.sync()
		let bobGroup = try await fixtures.bobClient.conversations.groups()[0]

		_ = try await aliceGroup.send(content: "sup gang original")
		_ = try await aliceGroup.send(content: "sup gang")

		try await aliceGroup.sync()
		let aliceGroupsCount = try await aliceGroup.decryptedMessages().count
		XCTAssertEqual(3, aliceGroupsCount)
		let aliceMessage = try await aliceGroup.decryptedMessages().first!

		try await bobGroup.sync()
		let bobGroupsCount = try await bobGroup.decryptedMessages().count
		XCTAssertEqual(2, bobGroupsCount)
		let bobMessage = try await bobGroup.decryptedMessages().first!

		XCTAssertEqual("sup gang", String(data: Data(aliceMessage.encodedContent.content), encoding: .utf8))
		XCTAssertEqual("sup gang", String(data: Data(bobMessage.encodedContent.content), encoding: .utf8))
	}
	
	func testCanStreamGroups() async throws {
		let fixtures = try await localFixtures()

		let expectation1 = expectation(description: "got a group")

		Task(priority: .userInitiated) {
			for try await _ in try await fixtures.aliceClient.conversations.streamGroups() {
				expectation1.fulfill()
			}
		}

		_ = try await fixtures.bobClient.conversations.newGroup(with: [fixtures.alice.address])

		await waitForExpectations(timeout: 3)
	}
	
	func testCanStreamGroupsAndConversationsWorksGroups() async throws {
		let fixtures = try await localFixtures()

		let expectation1 = expectation(description: "got a conversation")
		expectation1.expectedFulfillmentCount = 2

		Task(priority: .userInitiated) {
			for try await _ in try await fixtures.aliceClient.conversations.streamAll() {
				expectation1.fulfill()
			}
		}

		_ = try await fixtures.bobClient.conversations.newGroup(with: [fixtures.alice.address])
		_ = try await fixtures.bobClient.conversations.newConversation(with: fixtures.alice.address)

		await waitForExpectations(timeout: 3)
	}
	
	func testCanStreamAllMessages() async throws {
		let fixtures = try await localFixtures()

		let expectation1 = expectation(description: "got a conversation")
		expectation1.expectedFulfillmentCount = 4
		let convo = try await fixtures.bobClient.conversations.newConversation(with: fixtures.alice.address)
		let group = try await fixtures.bobClient.conversations.newGroup(with: [fixtures.alice.address])
		try await fixtures.aliceClient.conversations.sync()
		Task(priority: .userInitiated) {
			for try await _ in try await fixtures.aliceClient.conversations.streamAllMessages(includeGroups: true) {
				expectation1.fulfill()
			}
		}

		_ = try await group.send(content: "hi")
		_ = try await convo.send(content: "hi")
		let group2 = try await fixtures.fredClient.conversations.newGroup(with: [fixtures.alice.address])
		let convo2 = try await fixtures.fredClient.conversations.newConversation(with: fixtures.alice.address)

		_ = try await group2.send(content: "hi")
		_ = try await convo2.send(content: "hi")

		await waitForExpectations(timeout: 3)
	}
	
	func testCanStreamAllDecryptedMessages() async throws {
		let fixtures = try await localFixtures()

		let expectation1 = expectation(description: "got a conversation")
		expectation1.expectedFulfillmentCount = 4
		let convo = try await fixtures.bobClient.conversations.newConversation(with: fixtures.alice.address)
		let group = try await fixtures.bobClient.conversations.newGroup(with: [fixtures.alice.address])
		try await fixtures.aliceClient.conversations.sync()
		Task(priority: .userInitiated) {
			for try await _ in try await fixtures.aliceClient.conversations.streamAllDecryptedMessages(includeGroups: true) {
				expectation1.fulfill()
			}
		}

		_ = try await group.send(content: "hi")
		_ = try await convo.send(content: "hi")
		
		let group2 = try await fixtures.fredClient.conversations.newGroup(with: [fixtures.alice.address])
		let convo2 = try await fixtures.fredClient.conversations.newConversation(with: fixtures.alice.address)

		_ = try await group2.send(content: "hi")
		_ = try await convo2.send(content: "hi")

		await waitForExpectations(timeout: 3)
	}
	
	func testCanStreamAllGroupMessages() async throws {
		let fixtures = try await localFixtures()

		let expectation1 = expectation(description: "got a conversation")

		let group = try await fixtures.bobClient.conversations.newGroup(with: [fixtures.alice.address])
		try await fixtures.aliceClient.conversations.sync()
		Task(priority: .userInitiated) {
			for try await _ in try await fixtures.aliceClient.conversations.streamAllGroupMessages() {
				expectation1.fulfill()
			}
		}

		_ = try await group.send(content: "hi")

		await waitForExpectations(timeout: 3)
	}
	
	func testCanStreamAllGroupDecryptedMessages() async throws {
		let fixtures = try await localFixtures()

		let expectation1 = expectation(description: "got a conversation")
		let group = try await fixtures.bobClient.conversations.newGroup(with: [fixtures.alice.address])
		try await fixtures.aliceClient.conversations.sync()
		Task(priority: .userInitiated) {
			for try await _ in try await fixtures.aliceClient.conversations.streamAllGroupDecryptedMessages() {
				expectation1.fulfill()
			}
		}

		_ = try await group.send(content: "hi")

		await waitForExpectations(timeout: 3)
	}
}
