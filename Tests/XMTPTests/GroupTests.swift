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
		let key = try Crypto.secureRandomBytes(count: 32)
		let alice = try PrivateKey.generate()
		let aliceClient = try await Client.create(
			account: alice,
			options: .init(
				api: .init(env: .local, isSecure: false),
				codecs: [GroupMembershipChangedCodec()],
				mlsAlpha: true,
				mlsEncryptionKey: key
			)
		)
		let bob = try PrivateKey.generate()
		let bobClient = try await Client.create(
			account: bob,
			options: .init(
				api: .init(env: .local, isSecure: false),
				codecs: [GroupMembershipChangedCodec()],
				mlsAlpha: true,
				mlsEncryptionKey: key
			)
		)
		let fred = try PrivateKey.generate()
		let fredClient = try await Client.create(
			account: fred,
			options: .init(
				api: .init(env: .local, isSecure: false),
				codecs: [GroupMembershipChangedCodec()],
				mlsAlpha: true,
				mlsEncryptionKey: key
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

	func testAddedByAddress() async throws {
		// Create clients
		let fixtures = try await localFixtures()

		// Alice creates a group and adds Bob to the group
		_ = try await fixtures.aliceClient.conversations.newGroup(with: [fixtures.bob.address])

		// Bob syncs groups - this will decrypt the Welcome and then
		// identify who added Bob to the group
		try await fixtures.bobClient.conversations.sync()
		
		// Check Bob's group for the added_by_address of the inviter
		let bobGroup = try await fixtures.bobClient.conversations.groups().first
		let aliceAddress = fixtures.alice.address.localizedLowercase
		let whoAddedBob = try bobGroup?.addedByAddress().localizedLowercase
		
		// Verify the welcome host_credential is equal to Amal's
		XCTAssertEqual(aliceAddress, whoAddedBob)
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
		let membershipChange = GroupMembershipChanges()

		try await fixtures.bobClient.conversations.sync()
		let bobGroup = try await fixtures.bobClient.conversations.groups()[0]

		_ = try await aliceGroup.send(content: "sup gang original")
		let messageId = try await aliceGroup.send(content: "sup gang")
		_ = try await aliceGroup.send(content: membershipChange, options: SendOptions(contentType: ContentTypeGroupMembershipChanged))

		try await aliceGroup.sync()
		let aliceGroupsCount = try await aliceGroup.messages().count
		XCTAssertEqual(3, aliceGroupsCount)
		let aliceMessage = try await aliceGroup.messages().first!

		try await bobGroup.sync()
		let bobGroupsCount = try await bobGroup.messages().count
		XCTAssertEqual(2, bobGroupsCount)
		let bobMessage = try await bobGroup.messages().first!

		XCTAssertEqual("sup gang", try aliceMessage.content())
		XCTAssertEqual(messageId, aliceMessage.id)
		XCTAssertEqual(.published, aliceMessage.deliveryStatus)
		XCTAssertEqual("sup gang", try bobMessage.content())
	}
	
	func testCanListGroupMessages() async throws {
		let fixtures = try await localFixtures()
		let aliceGroup = try await fixtures.aliceClient.conversations.newGroup(with: [fixtures.bob.address])
		_ = try await aliceGroup.send(content: "howdy")
		_ = try await aliceGroup.send(content: "gm")

		var aliceMessagesCount = try await aliceGroup.messages().count
		var aliceMessagesUnpublishedCount = try await aliceGroup.messages(deliveryStatus: .unpublished).count
		var aliceMessagesPublishedCount = try await aliceGroup.messages(deliveryStatus: .published).count
		XCTAssertEqual(3, aliceMessagesCount)
		XCTAssertEqual(2, aliceMessagesUnpublishedCount)
		XCTAssertEqual(1, aliceMessagesPublishedCount)

		try await aliceGroup.sync()
		
		aliceMessagesCount = try await aliceGroup.messages().count
		aliceMessagesUnpublishedCount = try await aliceGroup.messages(deliveryStatus: .unpublished).count
		aliceMessagesPublishedCount = try await aliceGroup.messages(deliveryStatus: .published).count
		XCTAssertEqual(3, aliceMessagesCount)
		XCTAssertEqual(0, aliceMessagesUnpublishedCount)
		XCTAssertEqual(3, aliceMessagesPublishedCount)

		try await fixtures.bobClient.conversations.sync()
		let bobGroup = try await fixtures.bobClient.conversations.groups()[0]
		try await bobGroup.sync()
		
		var bobMessagesCount = try await bobGroup.messages().count
		var bobMessagesUnpublishedCount = try await bobGroup.messages(deliveryStatus: .unpublished).count
		var bobMessagesPublishedCount = try await bobGroup.messages(deliveryStatus: .published).count
		XCTAssertEqual(2, bobMessagesCount)
		XCTAssertEqual(0, bobMessagesUnpublishedCount)
		XCTAssertEqual(2, bobMessagesPublishedCount)

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
	
	func testCanStreamGroupMessages() async throws {
		let fixtures = try await localFixtures()
		let group = try await fixtures.bobClient.conversations.newGroup(with: [fixtures.alice.address])
		let membershipChange = GroupMembershipChanges()
		let expectation1 = expectation(description: "got a message")
		expectation1.expectedFulfillmentCount = 1

		Task(priority: .userInitiated) {
			for try await _ in group.streamMessages() {
				expectation1.fulfill()
			}
		}

		_ = try await group.send(content: "hi")
		_ = try await group.send(content: membershipChange, options: SendOptions(contentType: ContentTypeGroupMembershipChanged))

		await waitForExpectations(timeout: 3)
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
		expectation1.expectedFulfillmentCount = 2
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

		await waitForExpectations(timeout: 3)
	}
	
	func testCanStreamAllDecryptedMessages() async throws {
		let fixtures = try await localFixtures()
		let membershipChange = GroupMembershipChanges()

		let expectation1 = expectation(description: "got a conversation")
		expectation1.expectedFulfillmentCount = 2
		let convo = try await fixtures.bobClient.conversations.newConversation(with: fixtures.alice.address)
		let group = try await fixtures.bobClient.conversations.newGroup(with: [fixtures.alice.address])
		try await fixtures.aliceClient.conversations.sync()
		Task(priority: .userInitiated) {
			for try await _ in try await fixtures.aliceClient.conversations.streamAllDecryptedMessages(includeGroups: true) {
				expectation1.fulfill()
			}
		}

		_ = try await group.send(content: "hi")
		_ = try await group.send(content: membershipChange, options: SendOptions(contentType: ContentTypeGroupMembershipChanged))
		_ = try await convo.send(content: "hi")

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
    
    func testCanUpdateGroupName() async throws {
        let fixtures = try await localFixtures()
        let group = try await fixtures.aliceClient.conversations.newGroup(with: [fixtures.bob.address])
        
        var groupName = try group.groupName()
        
        XCTAssertEqual(groupName, "New Group")

        try await group.updateGroupName(groupName: "Test Group Name 1")
        
        groupName = try group.groupName()
        
        XCTAssertEqual(groupName, "Test Group Name 1")
        
        let bobConv = try await fixtures.bobClient.conversations.list(includeGroups: true)[0]
        let bobGroup: Group;
        switch bobConv {
            case .v1(_):
                XCTFail("failed converting conversation to group")
                return
            case .v2(_):
                XCTFail("failed converting conversation to group")
                return
            case .group(let group):
                bobGroup = group
        }
        groupName = try bobGroup.groupName()
        XCTAssertEqual(groupName, "New Group")
        
        try await bobGroup.sync()
        groupName = try bobGroup.groupName()
        
        XCTAssertEqual(groupName, "Test Group Name 1")
    }
}
