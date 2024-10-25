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
		var davonV3: PrivateKey!
		var aliceClient: Client!
		var bobClient: Client!
		var fredClient: Client!
		var davonV3Client: Client!
	}

	func localFixtures() async throws -> LocalFixtures {
		let key = try Crypto.secureRandomBytes(count: 32)
		let options = ClientOptions.init(
			api: .init(env: .local, isSecure: false),
			   codecs: [GroupUpdatedCodec()],
			   enableV3: true,
			   encryptionKey: key
		   )
		let alice = try PrivateKey.generate()
		let aliceClient = try await Client.create(
			account: alice,
			options: options
		)
		let bob = try PrivateKey.generate()
		let bobClient = try await Client.create(
			account: bob,
			options: options
		)
		let fred = try PrivateKey.generate()
		let fredClient = try await Client.create(
			account: fred,
			options: options
		)
		
		let davonV3 = try PrivateKey.generate()
		let davonV3Client = try await Client.createV3(
			account: davonV3,
			options: options
		)

		return .init(
			alice: alice,
			bob: bob,
			fred: fred,
			davonV3: davonV3,
			aliceClient: aliceClient,
			bobClient: bobClient,
			fredClient: fredClient,
			davonV3Client: davonV3Client
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

		var aliceMembersCount = try await aliceGroup.members.count
		var bobMembersCount = try await bobGroup.members.count
		XCTAssertEqual(aliceMembersCount, 3)
		XCTAssertEqual(bobMembersCount, 3)
        
        try await bobGroup.addAdmin(inboxId: fixtures.aliceClient.inboxID)

		try await aliceGroup.removeMembers(addresses: [fixtures.fred.address])
		try await bobGroup.sync()

		aliceMembersCount = try await aliceGroup.members.count
		bobMembersCount = try await bobGroup.members.count
        XCTAssertEqual(aliceMembersCount, 2)
		XCTAssertEqual(bobMembersCount, 2)

		try await bobGroup.addMembers(addresses: [fixtures.fred.address])
		try await aliceGroup.sync()
        
        try await bobGroup.removeAdmin(inboxId: fixtures.aliceClient.inboxID)
        try await aliceGroup.sync()

		aliceMembersCount = try await aliceGroup.members.count
		bobMembersCount = try await bobGroup.members.count
		XCTAssertEqual(aliceMembersCount, 3)
		XCTAssertEqual(bobMembersCount, 3)
		
        XCTAssertEqual(try bobGroup.permissionPolicySet().addMemberPolicy, .allow)
		XCTAssertEqual(try aliceGroup.permissionPolicySet().addMemberPolicy, .allow)

        XCTAssert(try bobGroup.isSuperAdmin(inboxId: fixtures.bobClient.inboxID))
        XCTAssert(try !bobGroup.isSuperAdmin(inboxId: fixtures.aliceClient.inboxID))
        XCTAssert(try aliceGroup.isSuperAdmin(inboxId: fixtures.bobClient.inboxID))
        XCTAssert(try !aliceGroup.isSuperAdmin(inboxId: fixtures.aliceClient.inboxID))
		
	}

	func testCanCreateAGroupWithAdminPermissions() async throws {
		let fixtures = try await localFixtures()
		let bobGroup = try await fixtures.bobClient.conversations.newGroup(with: [fixtures.alice.address], permissions: GroupPermissionPreconfiguration.adminOnly)
		try await fixtures.aliceClient.conversations.sync()
		let aliceGroup = try await fixtures.aliceClient.conversations.groups().first!
		XCTAssert(!bobGroup.id.isEmpty)
		XCTAssert(!aliceGroup.id.isEmpty)

		let bobConsentResult = try await fixtures.bobClient.contacts.consentList.groupState(groupId: bobGroup.id)
		XCTAssertEqual(bobConsentResult, ConsentState.allowed)

		let aliceConsentResult = try await fixtures.aliceClient.contacts.consentList.groupState(groupId: aliceGroup.id)
		XCTAssertEqual(aliceConsentResult, ConsentState.unknown)

		try await bobGroup.addMembers(addresses: [fixtures.fred.address])
		try await aliceGroup.sync()

		var aliceMembersCount = try await aliceGroup.members.count
		var bobMembersCount = try await bobGroup.members.count
		XCTAssertEqual(aliceMembersCount, 3)
		XCTAssertEqual(bobMembersCount, 3)

		await assertThrowsAsyncError(
			try await aliceGroup.removeMembers(addresses: [fixtures.fred.address])
		)
		try await bobGroup.sync()

		aliceMembersCount = try await aliceGroup.members.count
		bobMembersCount = try await bobGroup.members.count
		XCTAssertEqual(aliceMembersCount, 3)
		XCTAssertEqual(bobMembersCount, 3)
		
		try await bobGroup.removeMembers(addresses: [fixtures.fred.address])
		try await aliceGroup.sync()

		aliceMembersCount = try await aliceGroup.members.count
		bobMembersCount = try await bobGroup.members.count
		XCTAssertEqual(aliceMembersCount, 2)
		XCTAssertEqual(bobMembersCount, 2)

		await assertThrowsAsyncError(
			try await aliceGroup.addMembers(addresses: [fixtures.fred.address])
		)
		try await bobGroup.sync()

		aliceMembersCount = try await aliceGroup.members.count
		bobMembersCount = try await bobGroup.members.count
		XCTAssertEqual(aliceMembersCount, 2)
		XCTAssertEqual(bobMembersCount, 2)
		
        XCTAssertEqual(try bobGroup.permissionPolicySet().addMemberPolicy, .admin)
        XCTAssertEqual(try aliceGroup.permissionPolicySet().addMemberPolicy, .admin)
        XCTAssert(try bobGroup.isSuperAdmin(inboxId: fixtures.bobClient.inboxID))
        XCTAssert(try !bobGroup.isSuperAdmin(inboxId: fixtures.aliceClient.inboxID))
        XCTAssert(try aliceGroup.isSuperAdmin(inboxId: fixtures.bobClient.inboxID))
        XCTAssert(try !aliceGroup.isSuperAdmin(inboxId: fixtures.aliceClient.inboxID))
	}

	func testCanListGroups() async throws {
		let fixtures = try await localFixtures()
		_ = try await fixtures.aliceClient.conversations.newGroup(with: [fixtures.bob.address])
		_ = try await fixtures.davonV3Client.conversations.findOrCreateDm(with: fixtures.bob.address)
		_ = try await fixtures.davonV3Client.conversations.findOrCreateDm(with: fixtures.alice.address)
		
		try await fixtures.aliceClient.conversations.sync()
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
		_ = try await fixtures.davonV3Client.conversations.findOrCreateDm(with: fixtures.bob.walletAddress)
		_ = try await fixtures.davonV3Client.conversations.findOrCreateDm(with: fixtures.alice.walletAddress)

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
		let members = try await group.members.map(\.inboxId).sorted()
		let peerMembers = try await group.peerInboxIds.sorted()

		XCTAssertEqual([fixtures.bobClient.inboxID, fixtures.aliceClient.inboxID].sorted(), members)
		XCTAssertEqual([fixtures.bobClient.inboxID].sorted(), peerMembers)
	}

	func testCanAddGroupMembers() async throws {
		let fixtures = try await localFixtures()
		let group = try await fixtures.aliceClient.conversations.newGroup(with: [fixtures.bob.address])

		try await group.addMembers(addresses: [fixtures.fred.address])

		try await group.sync()
		let members = try await group.members.map(\.inboxId).sorted()

		XCTAssertEqual([
			fixtures.bobClient.inboxID,
			fixtures.aliceClient.inboxID,
			fixtures.fredClient.inboxID
		].sorted(), members)

		let groupChangedMessage: GroupUpdated = try await group.messages().first!.content()
		XCTAssertEqual(groupChangedMessage.addedInboxes.map(\.inboxID), [fixtures.fredClient.inboxID])
	}
	
	func testCanAddGroupMembersByInboxId() async throws {
		let fixtures = try await localFixtures()
		let group = try await fixtures.aliceClient.conversations.newGroup(with: [fixtures.bob.address])

		try await group.addMembersByInboxId(inboxIds: [fixtures.fredClient.inboxID])

		try await group.sync()
		let members = try await group.members.map(\.inboxId).sorted()

		XCTAssertEqual([
			fixtures.bobClient.inboxID,
			fixtures.aliceClient.inboxID,
			fixtures.fredClient.inboxID
		].sorted(), members)

		let groupChangedMessage: GroupUpdated = try await group.messages().first!.content()
		XCTAssertEqual(groupChangedMessage.addedInboxes.map(\.inboxID), [fixtures.fredClient.inboxID])
	}

	func testCanRemoveMembers() async throws {
		let fixtures = try await localFixtures()
		let group = try await fixtures.aliceClient.conversations.newGroup(with: [fixtures.bob.address, fixtures.fred.address])

		try await group.sync()
		let members = try await group.members.map(\.inboxId).sorted()

		XCTAssertEqual([
			fixtures.bobClient.inboxID,
			fixtures.aliceClient.inboxID,
			fixtures.fredClient.inboxID
		].sorted(), members)

		try await group.removeMembers(addresses: [fixtures.fred.address])

		try await group.sync()

		let newMembers = try await group.members.map(\.inboxId).sorted()
		XCTAssertEqual([
			fixtures.bobClient.inboxID,
			fixtures.aliceClient.inboxID,
		].sorted(), newMembers)

		let groupChangedMessage: GroupUpdated = try await group.messages().first!.content()
		XCTAssertEqual(groupChangedMessage.removedInboxes.map(\.inboxID), [fixtures.fredClient.inboxID])
	}
	
	func testCanRemoveMembersByInboxId() async throws {
		let fixtures = try await localFixtures()
		let group = try await fixtures.aliceClient.conversations.newGroup(with: [fixtures.bob.address, fixtures.fred.address])

		try await group.sync()
		let members = try await group.members.map(\.inboxId).sorted()

		XCTAssertEqual([
			fixtures.bobClient.inboxID,
			fixtures.aliceClient.inboxID,
			fixtures.fredClient.inboxID
		].sorted(), members)

		try await group.removeMembersByInboxId(inboxIds: [fixtures.fredClient.inboxID])

		try await group.sync()

		let newMembers = try await group.members.map(\.inboxId).sorted()
		XCTAssertEqual([
			fixtures.bobClient.inboxID,
			fixtures.aliceClient.inboxID,
		].sorted(), newMembers)

		let groupChangedMessage: GroupUpdated = try await group.messages().first!.content()
		XCTAssertEqual(groupChangedMessage.removedInboxes.map(\.inboxID), [fixtures.fredClient.inboxID])
	}
	
	func testCanMessage() async throws {
		let fixtures = try await localFixtures()
		let notOnNetwork = try PrivateKey.generate()
		let canMessage = try await fixtures.aliceClient.canMessageV3(address: fixtures.bobClient.address)
		let cannotMessage = try await fixtures.aliceClient.canMessageV3(addresses: [notOnNetwork.address, fixtures.bobClient.address])
		XCTAssert(canMessage)
		XCTAssert(!(cannotMessage[notOnNetwork.address.lowercased()] ?? true))
	}
	
	func testIsActive() async throws {
		let fixtures = try await localFixtures()
		let group = try await fixtures.aliceClient.conversations.newGroup(with: [fixtures.bob.address, fixtures.fred.address])

		try await group.sync()
		let members = try await group.members.map(\.inboxId).sorted()

		XCTAssertEqual([
			fixtures.bobClient.inboxID,
			fixtures.aliceClient.inboxID,
			fixtures.fredClient.inboxID
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

		let newMembers = try await group.members.map(\.inboxId).sorted()
		XCTAssertEqual([
			fixtures.bobClient.inboxID,
			fixtures.aliceClient.inboxID,
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
		let aliceAddress = fixtures.aliceClient.inboxID
		let whoAddedBob = try bobGroup?.addedByInboxId()
		
		// Verify the welcome host_credential is equal to Amal's
		XCTAssertEqual(aliceAddress, whoAddedBob)
	}

	func testCannotStartGroupWithSelf() async throws {
		let fixtures = try await localFixtures()

		await assertThrowsAsyncError(
			try await fixtures.aliceClient.conversations.newGroup(with: [fixtures.alice.address])
		)
	}

	func testCanStartEmptyGroup() async throws {
		let fixtures = try await localFixtures()
		let group = try await fixtures.aliceClient.conversations.newGroup(with: [])
		XCTAssert(!group.id.isEmpty)
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

		let isGroupAllowedResult = try await fixtures.bobClient.contacts.isGroupAllowed(groupId: bobGroup.id)
		XCTAssertTrue(isGroupAllowedResult)

		let groupStateResult = try await fixtures.bobClient.contacts.consentList.groupState(groupId: bobGroup.id)
		XCTAssertEqual(groupStateResult, ConsentState.allowed)
	}
	
	func testCanSendMessagesToGroup() async throws {
		let fixtures = try await localFixtures()
		let aliceGroup = try await fixtures.aliceClient.conversations.newGroup(with: [fixtures.bob.address])
		let membershipChange = GroupUpdated()

		try await fixtures.bobClient.conversations.sync()
		let bobGroup = try await fixtures.bobClient.conversations.groups()[0]

		_ = try await aliceGroup.send(content: "sup gang original")
		let messageId = try await aliceGroup.send(content: "sup gang")
		_ = try await aliceGroup.send(content: membershipChange, options: SendOptions(contentType: ContentTypeGroupUpdated))

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
		var aliceMessagesPublishedCount = try await aliceGroup.messages(deliveryStatus: .published).count
		XCTAssertEqual(3, aliceMessagesCount)
		XCTAssertEqual(3, aliceMessagesPublishedCount)

		try await aliceGroup.sync()
		
		aliceMessagesCount = try await aliceGroup.messages().count
		let aliceMessagesUnpublishedCount = try await aliceGroup.messages(deliveryStatus: .unpublished).count
		aliceMessagesPublishedCount = try await aliceGroup.messages(deliveryStatus: .published).count
		XCTAssertEqual(3, aliceMessagesCount)
		XCTAssertEqual(0, aliceMessagesUnpublishedCount)
		XCTAssertEqual(3, aliceMessagesPublishedCount)

		try await fixtures.bobClient.conversations.sync()
		let bobGroup = try await fixtures.bobClient.conversations.groups()[0]
		try await bobGroup.sync()
		
		let bobMessagesCount = try await bobGroup.messages().count
        let bobMessagesUnpublishedCount = try await bobGroup.messages(deliveryStatus: .unpublished).count
		let bobMessagesPublishedCount = try await bobGroup.messages(deliveryStatus: .published).count
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
		let membershipChange = GroupUpdated()
		let expectation1 = XCTestExpectation(description: "got a message")
		expectation1.expectedFulfillmentCount = 1

		Task(priority: .userInitiated) {
			for try await _ in group.streamMessages() {
				expectation1.fulfill()
			}
		}

		_ = try await group.send(content: "hi")
		_ = try await group.send(content: membershipChange, options: SendOptions(contentType: ContentTypeGroupUpdated))

		await fulfillment(of: [expectation1], timeout: 3)
	}
	
	func testCanStreamGroups() async throws {
		let fixtures = try await localFixtures()

		let expectation1 = XCTestExpectation(description: "got a group")

		Task(priority: .userInitiated) {
			for try await _ in try await fixtures.aliceClient.conversations.streamGroups() {
				expectation1.fulfill()
			}
		}

		_ = try await fixtures.bobClient.conversations.newGroup(with: [fixtures.alice.address])
		_ = try await fixtures.davonV3Client.conversations.findOrCreateDm(with: fixtures.alice.address)

		await fulfillment(of: [expectation1], timeout: 3)
	}
	
	func testCanStreamGroupsAndConversationsWorksGroups() async throws {
		let fixtures = try await localFixtures()

		let expectation1 = XCTestExpectation(description: "got a conversation")
		expectation1.expectedFulfillmentCount = 2

		Task(priority: .userInitiated) {
			for try await _ in await fixtures.aliceClient.conversations.streamAll() {
				expectation1.fulfill()
			}
		}

		_ = try await fixtures.bobClient.conversations.newGroup(with: [fixtures.alice.address])
		_ = try await fixtures.bobClient.conversations.newConversation(with: fixtures.alice.address)
		_ = try await fixtures.davonV3Client.conversations.findOrCreateDm(with: fixtures.alice.address)

		await fulfillment(of: [expectation1], timeout: 3)
	}
	
	func testStreamGroupsAndAllMessages() async throws {
		let fixtures = try await localFixtures()
		
		let expectation1 = XCTestExpectation(description: "got a group")
		let expectation2 = XCTestExpectation(description: "got a message")


		Task(priority: .userInitiated) {
			for try await _ in try await fixtures.aliceClient.conversations.streamGroups() {
				expectation1.fulfill()
			}
		}
		
		Task(priority: .userInitiated) {
			for try await _ in await fixtures.aliceClient.conversations.streamAllMessages(includeGroups: true) {
				expectation2.fulfill()
			}
		}

		let group = try await fixtures.bobClient.conversations.newGroup(with: [fixtures.alice.address])
		_ = try await group.send(content: "hello")

		await fulfillment(of: [expectation1, expectation2], timeout: 3)
	}
	
	func testCanStreamAndUpdateNameWithoutForkingGroup() async throws {
		let fixtures = try await localFixtures()
		
		let expectation = XCTestExpectation(description: "got a message")
		expectation.expectedFulfillmentCount = 5

		Task(priority: .userInitiated) {
			for try await _ in await fixtures.bobClient.conversations.streamAllGroupMessages(){
				expectation.fulfill()
			}
		}

		let alixGroup = try await fixtures.aliceClient.conversations.newGroup(with: [fixtures.bob.address])
		try await alixGroup.updateGroupName(groupName: "hello")
		_ = try await alixGroup.send(content: "hello1")
		
		try await fixtures.bobClient.conversations.sync()

		let boGroups = try await fixtures.bobClient.conversations.groups()
		XCTAssertEqual(boGroups.count, 1, "bo should have 1 group")
		let boGroup = boGroups[0]
		try await boGroup.sync()
		
		let boMessages1 = try await boGroup.messages()
		XCTAssertEqual(boMessages1.count, 2, "should have 2 messages on first load received \(boMessages1.count)")
		
		_ = try await boGroup.send(content: "hello2")
		_ = try await boGroup.send(content: "hello3")
		try await alixGroup.sync()

		let alixMessages = try await alixGroup.messages()
		for message in alixMessages {
			print("message", message.encodedContent.type, message.encodedContent.type.typeID)
		}
		XCTAssertEqual(alixMessages.count, 5, "should have 5 messages on first load received \(alixMessages.count)")

		_ = try await alixGroup.send(content: "hello4")
		try await boGroup.sync()

		let boMessages2 = try await boGroup.messages()
		for message in boMessages2 {
			print("message", message.encodedContent.type, message.encodedContent.type.typeID)
		}
		XCTAssertEqual(boMessages2.count, 5, "should have 5 messages on second load received \(boMessages2.count)")

		await fulfillment(of: [expectation], timeout: 3)
	}
	
	func testCanStreamAllMessages() async throws {
		let fixtures = try await localFixtures()

		let expectation1 = XCTestExpectation(description: "got a conversation")
		expectation1.expectedFulfillmentCount = 2
		let convo = try await fixtures.bobClient.conversations.newConversation(with: fixtures.alice.address)
		let group = try await fixtures.bobClient.conversations.newGroup(with: [fixtures.alice.address])
		let dm = try await fixtures.davonV3Client.conversations.findOrCreateDm(with: fixtures.alice.address)

		try await fixtures.aliceClient.conversations.sync()
		Task(priority: .userInitiated) {
			for try await _ in try await fixtures.aliceClient.conversations.streamAllMessages(includeGroups: true) {
				expectation1.fulfill()
			}
		}

		_ = try await group.send(content: "hi")
		_ = try await convo.send(content: "hi")
		_ = try await dm.send(content: "hi")

		await fulfillment(of: [expectation1], timeout: 3)
	}
	
	func testCanStreamAllDecryptedMessages() async throws {
		let fixtures = try await localFixtures()
		let membershipChange = GroupUpdated()

		let expectation1 = XCTestExpectation(description: "got a conversation")
		expectation1.expectedFulfillmentCount = 2
		let convo = try await fixtures.bobClient.conversations.newConversation(with: fixtures.alice.address)
		let group = try await fixtures.bobClient.conversations.newGroup(with: [fixtures.alice.address])
		let dm = try await fixtures.davonV3Client.conversations.findOrCreateDm(with: fixtures.alice.address)
		try await fixtures.aliceClient.conversations.sync()
		Task(priority: .userInitiated) {
			for try await _ in await fixtures.aliceClient.conversations.streamAllDecryptedMessages(includeGroups: true) {
				expectation1.fulfill()
			}
		}

		_ = try await group.send(content: "hi")
		_ = try await group.send(content: membershipChange, options: SendOptions(contentType: ContentTypeGroupUpdated))
		_ = try await convo.send(content: "hi")
		_ = try await dm.send(content: "hi")

		await fulfillment(of: [expectation1], timeout: 3)
	}
	
	func testCanStreamAllGroupMessages() async throws {
		let fixtures = try await localFixtures()

		let expectation1 = XCTestExpectation(description: "got a conversation")

		let group = try await fixtures.bobClient.conversations.newGroup(with: [fixtures.alice.address])
		let dm = try await fixtures.davonV3Client.conversations.findOrCreateDm(with: fixtures.alice.address)
		try await fixtures.aliceClient.conversations.sync()
		Task(priority: .userInitiated) {
			for try await _ in await fixtures.aliceClient.conversations.streamAllGroupMessages() {
				expectation1.fulfill()
			}
		}

		_ = try await group.send(content: "hi")
		_ = try await dm.send(content: "hi")

		await fulfillment(of: [expectation1], timeout: 3)
	}
	
	func testCanStreamAllGroupDecryptedMessages() async throws {
		let fixtures = try await localFixtures()

		let expectation1 = XCTestExpectation(description: "got a conversation")
		let group = try await fixtures.bobClient.conversations.newGroup(with: [fixtures.alice.address])
		let dm = try await fixtures.davonV3Client.conversations.findOrCreateDm(with: fixtures.alice.address)

		try await fixtures.aliceClient.conversations.sync()
		Task(priority: .userInitiated) {
			for try await _ in await fixtures.aliceClient.conversations.streamAllGroupDecryptedMessages() {
				expectation1.fulfill()
			}
		}

		_ = try await group.send(content: "hi")
		_ = try await dm.send(content: "hi")

		await fulfillment(of: [expectation1], timeout: 3)
	}
    
    func testCanUpdateGroupMetadata() async throws {
        let fixtures = try await localFixtures()
        let group = try await fixtures.aliceClient.conversations.newGroup(with: [fixtures.bob.address], name: "Start Name", imageUrlSquare: "starturl.com")
        
        var groupName = try group.groupName()
		var groupImageUrlSquare = try group.groupImageUrlSquare()
        
        XCTAssertEqual(groupName, "Start Name")
		XCTAssertEqual(groupImageUrlSquare, "starturl.com")


        try await group.updateGroupName(groupName: "Test Group Name 1")
		try await group.updateGroupImageUrlSquare(imageUrlSquare: "newurl.com")
        
        groupName = try group.groupName()
		groupImageUrlSquare = try group.groupImageUrlSquare()

        XCTAssertEqual(groupName, "Test Group Name 1")
		XCTAssertEqual(groupImageUrlSquare, "newurl.com")
		
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
		    case .dm(_):
				XCTFail("failed converting conversation to group")
				return
		}
        groupName = try bobGroup.groupName()
        XCTAssertEqual(groupName, "Start Name")
        
        try await bobGroup.sync()
        groupName = try bobGroup.groupName()
		groupImageUrlSquare = try bobGroup.groupImageUrlSquare()
		
		XCTAssertEqual(groupImageUrlSquare, "newurl.com")
        XCTAssertEqual(groupName, "Test Group Name 1")
    }
	
	func testGroupConsent() async throws {
		let fixtures = try await localFixtures()
		let group = try await fixtures.bobClient.conversations.newGroup(with: [fixtures.alice.address])
		let isAllowed = try await fixtures.bobClient.contacts.isGroupAllowed(groupId: group.id)
		XCTAssert(isAllowed)
		XCTAssertEqual(try group.consentState(), .allowed)
		
		try await fixtures.bobClient.contacts.denyGroups(groupIds: [group.id])
		let isDenied = try await fixtures.bobClient.contacts.isGroupDenied(groupId: group.id)
		XCTAssert(isDenied)
		XCTAssertEqual(try group.consentState(), .denied)
		
		try await group.updateConsentState(state: .allowed)
		let isAllowed2 = try await fixtures.bobClient.contacts.isGroupAllowed(groupId: group.id)
		XCTAssert(isAllowed2)
		XCTAssertEqual(try group.consentState(), .allowed)
	}
	
	func testCanAllowAndDenyInboxId() async throws {
		let fixtures = try await localFixtures()
		let boGroup = try await fixtures.bobClient.conversations.newGroup(with: [fixtures.alice.address])
		var isInboxAllowed = try await fixtures.bobClient.contacts.isInboxAllowed(inboxId: fixtures.aliceClient.address)
		var isInboxDenied = try await fixtures.bobClient.contacts.isInboxDenied(inboxId: fixtures.aliceClient.address)
		XCTAssert(!isInboxAllowed)
		XCTAssert(!isInboxDenied)

		
		try await fixtures.bobClient.contacts.allowInboxes(inboxIds: [fixtures.aliceClient.inboxID])
		var alixMember = try await boGroup.members.first(where: { member in member.inboxId == fixtures.aliceClient.inboxID })
		XCTAssertEqual(alixMember?.consentState, .allowed)

		isInboxAllowed = try await fixtures.bobClient.contacts.isInboxAllowed(inboxId: fixtures.aliceClient.inboxID)
		XCTAssert(isInboxAllowed)
		isInboxDenied = try await fixtures.bobClient.contacts.isInboxDenied(inboxId: fixtures.aliceClient.inboxID)
		XCTAssert(!isInboxDenied)

		
		try await fixtures.bobClient.contacts.denyInboxes(inboxIds: [fixtures.aliceClient.inboxID])
		alixMember = try await boGroup.members.first(where: { member in member.inboxId == fixtures.aliceClient.inboxID })
		XCTAssertEqual(alixMember?.consentState, .denied)
		
		isInboxAllowed = try await fixtures.bobClient.contacts.isInboxAllowed(inboxId: fixtures.aliceClient.inboxID)
		isInboxDenied = try await fixtures.bobClient.contacts.isInboxDenied(inboxId: fixtures.aliceClient.inboxID)
		XCTAssert(!isInboxAllowed)
		XCTAssert(isInboxDenied)
		
		try await fixtures.bobClient.contacts.allow(addresses: [fixtures.aliceClient.address])
		let isAddressAllowed = try await fixtures.bobClient.contacts.isAllowed(fixtures.aliceClient.address)
		let isAddressDenied = try await fixtures.bobClient.contacts.isDenied(fixtures.aliceClient.address)
		XCTAssert(isAddressAllowed)
		XCTAssert(!isAddressDenied)
		isInboxAllowed = try await fixtures.bobClient.contacts.isInboxAllowed(inboxId: fixtures.aliceClient.inboxID)
		isInboxDenied = try await fixtures.bobClient.contacts.isInboxDenied(inboxId: fixtures.aliceClient.inboxID)
		XCTAssert(isInboxAllowed)
		XCTAssert(!isInboxDenied)
	}
	
	func testCanFetchGroupById() async throws {
		let fixtures = try await localFixtures()

		let boGroup = try await fixtures.bobClient.conversations.newGroup(with: [fixtures.alice.address])
		try await fixtures.aliceClient.conversations.sync()
		let alixGroup = try fixtures.aliceClient.findGroup(groupId: boGroup.id)

		XCTAssertEqual(alixGroup?.id, boGroup.id)
	}

	func testCanFetchMessageById() async throws {
		let fixtures = try await localFixtures()

		let boGroup = try await fixtures.bobClient.conversations.newGroup(with: [fixtures.alice.address])

		let boMessageId = try await boGroup.send(content: "Hello")
		try await fixtures.aliceClient.conversations.sync()
		let alixGroup = try fixtures.aliceClient.findGroup(groupId: boGroup.id)
		try await alixGroup?.sync()
		_ = try fixtures.aliceClient.findMessage(messageId: boMessageId)

		XCTAssertEqual(alixGroup?.id, boGroup.id)
	}
	
	func testUnpublishedMessages() async throws {
		let fixtures = try await localFixtures()
		let boGroup = try await fixtures.bobClient.conversations.newGroup(with: [fixtures.alice.address])

		try await fixtures.aliceClient.conversations.sync()
		let alixGroup = try fixtures.aliceClient.findGroup(groupId: boGroup.id)!
		let isGroupAllowed = try await fixtures.aliceClient.contacts.isGroupAllowed(groupId: boGroup.id)
		XCTAssert(!isGroupAllowed)
		let preparedMessageId = try await alixGroup.prepareMessage(content: "Test text")
		let isGroupAllowed2 = try await fixtures.aliceClient.contacts.isGroupAllowed(groupId: boGroup.id)
		XCTAssert(isGroupAllowed2)
		let messageCount = try await alixGroup.messages().count
		XCTAssertEqual(messageCount, 1)
		let messageCountPublished = try await alixGroup.messages(deliveryStatus: .published).count
		let messageCountUnpublished = try await alixGroup.messages(deliveryStatus: .unpublished).count
		XCTAssertEqual(messageCountPublished, 0)
		XCTAssertEqual(messageCountUnpublished, 1)

		_ = try await alixGroup.publishMessages()
		try await alixGroup.sync()

		let messageCountPublished2 = try await alixGroup.messages(deliveryStatus: .published).count
		let messageCountUnpublished2 = try await alixGroup.messages(deliveryStatus: .unpublished).count
		let messageCount2 = try await alixGroup.messages().count
		XCTAssertEqual(messageCountPublished2, 1)
		XCTAssertEqual(messageCountUnpublished2, 0)
		XCTAssertEqual(messageCount2, 1)

		let messages = try await alixGroup.messages()

		XCTAssertEqual(preparedMessageId, messages.first!.id)
	}
	
	func testCanSyncManyGroupsInUnderASecond() async throws {
		let fixtures = try await localFixtures()
		var groups: [Group] = []

		for _ in 0..<100 {
			let group = try await fixtures.aliceClient.conversations.newGroup(with: [fixtures.bob.address])
			groups.append(group)
		}
		try await fixtures.bobClient.conversations.sync()
		let bobGroup = try fixtures.bobClient.findGroup(groupId: groups[0].id)
		_ = try await groups[0].send(content: "hi")
		let messageCount = try await bobGroup!.messages().count
		XCTAssertEqual(messageCount, 0)
		do {
			let start = Date()
			let numGroupsSynced = try await fixtures.bobClient.conversations.syncAllGroups()
			let end = Date()
			print(end.timeIntervalSince(start))
			XCTAssert(end.timeIntervalSince(start) < 1)
            XCTAssert(numGroupsSynced == 100)
		} catch {
			print("Failed to list groups members: \(error)")
			throw error // Rethrow the error to fail the test if group creation fails
		}
		
		let messageCount2 = try await bobGroup!.messages().count
		XCTAssertEqual(messageCount2, 1)
        
        for aliceConv in try await fixtures.aliceClient.conversations.list(includeGroups: true) {
            guard case let .group(aliceGroup) = aliceConv else {
                   XCTFail("failed converting conversation to group")
                   return
               }
            try await aliceGroup.removeMembers(addresses: [fixtures.bobClient.address])
        }
        
        // first syncAllGroups after removal still sync groups in order to process the removal
        var numGroupsSynced = try await fixtures.bobClient.conversations.syncAllGroups()
        XCTAssert(numGroupsSynced == 100)
        
        // next syncAllGroups only will sync active groups
        numGroupsSynced = try await fixtures.bobClient.conversations.syncAllGroups()
        XCTAssert(numGroupsSynced == 0)
	}
	
	func testCanListManyMembersInParallelInUnderASecond() async throws {
		let fixtures = try await localFixtures()
		var groups: [Group] = []

		for _ in 0..<100 {
			let group = try await fixtures.aliceClient.conversations.newGroup(with: [fixtures.bob.address])
			groups.append(group)
		}
		do {
			let start = Date()
			let _ = try await listMembersInParallel(groups: groups)
			let end = Date()
			print(end.timeIntervalSince(start))
			XCTAssert(end.timeIntervalSince(start) < 1)
		} catch {
			print("Failed to list groups members: \(error)")
			throw error // Rethrow the error to fail the test if group creation fails
		}
	}
	
	func listMembersInParallel(groups: [Group]) async throws {
		await withThrowingTaskGroup(of: [Member].self) { taskGroup in
			for group in groups {
				taskGroup.addTask {
					return try await group.members
				}
			}
		}
	}
	
	func testCanStreamAllDecryptedMessagesAndCancelStream() async throws {
		let fixtures = try await localFixtures()

		var messages = 0
		let messagesQueue = DispatchQueue(label: "messages.queue")  // Serial queue to synchronize access to `messages`

		let convo = try await fixtures.bobClient.conversations.newConversation(with: fixtures.alice.address)
		let group = try await fixtures.bobClient.conversations.newGroup(with: [fixtures.alice.address])
		try await fixtures.aliceClient.conversations.sync()

		let streamingTask = Task(priority: .userInitiated) {
			for try await _ in await fixtures.aliceClient.conversations.streamAllDecryptedMessages(includeGroups: true) {
				messagesQueue.sync {
					messages += 1
				}
			}
		}

		_ = try await group.send(content: "hi")
		_ = try await convo.send(content: "hi")

		try await Task.sleep(nanoseconds: 1_000_000_000)

		streamingTask.cancel()

		messagesQueue.sync {
			XCTAssertEqual(messages, 2)
		}
		
		try await Task.sleep(nanoseconds: 1_000_000_000)
		
		_ = try await group.send(content: "hi")
		_ = try await group.send(content: "hi")
		_ = try await group.send(content: "hi")
		_ = try await convo.send(content: "hi")
		
		try await Task.sleep(nanoseconds: 1_000_000_000)
		
		messagesQueue.sync {
			XCTAssertEqual(messages, 2)
		}
		
		let streamingTask2 = Task(priority: .userInitiated) {
			for try await _ in await fixtures.aliceClient.conversations.streamAllDecryptedMessages(includeGroups: true) {
				// Update the messages count in a thread-safe manner
				messagesQueue.sync {
					messages += 1
				}
			}
		}
		
		_ = try await group.send(content: "hi")
		_ = try await convo.send(content: "hi")
		
		try await Task.sleep(nanoseconds: 1_000_000_000)
		
		messagesQueue.sync {
			XCTAssertEqual(messages, 4)
		}
	}
}
