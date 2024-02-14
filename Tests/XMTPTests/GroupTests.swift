//
//  GroupTests.swift
//
//
//  Created by Pat Nakajima on 2/1/24.
//

import CryptoKit
import XCTest
@testable import XMTPiOS
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

	func testCanCreateGroups() async throws {
		let fixtures = try await localFixtures()
		let group = try await fixtures.aliceClient.conversations.newGroup(with: [fixtures.bob.address])

		XCTAssert(!group.id.isEmpty)
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

		XCTAssertEqual([fixtures.bob.address.localizedLowercase, fixtures.alice.address.localizedLowercase].sorted(), members)
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

		let groupChangedMessage: GroupMembershipChanges = try await group.messages().last!.content()
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

		let groupChangedMessage: GroupMembershipChanges = try await group.messages().last!.content()
		XCTAssertEqual(groupChangedMessage.membersRemoved.map(\.accountAddress.localizedLowercase), [fixtures.fred.address.localizedLowercase])
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

	func testCanSendMessagesToGroup() async throws {
		let fixtures = try await localFixtures()
		let aliceGroup = try await fixtures.aliceClient.conversations.newGroup(with: [fixtures.bob.address])

		try await fixtures.bobClient.conversations.sync()
		let bobGroup = try await fixtures.bobClient.conversations.groups()[0]

		try await aliceGroup.send(content: "sup gang original")
		try await aliceGroup.send(content: "sup gang")

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

		try await aliceGroup.send(content: "sup gang original")
		try await aliceGroup.send(content: "sup gang")

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
}
