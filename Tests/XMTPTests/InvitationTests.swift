//
//  InvitationTests.swift
//
//
//  Created by Pat Nakajima on 11/27/22.
//

import Foundation
import XCTest
@testable import XMTP
import XMTPTestHelpers

@available(iOS 16.0, *)
class InvitationTests: XCTestCase {

	func testDeterministicInvite() async throws {
		let aliceWallet = try FakeWallet.generate()
		let bobWallet = try FakeWallet.generate()

		let alice = try await PrivateKeyBundleV1.generate(wallet: aliceWallet)
		let bob = try await PrivateKeyBundleV1.generate(wallet: bobWallet)

		let makeInvite = { (conversationID: String) in
			try InvitationV1.createDeterministic(
					sender: alice.toV2(),
					recipient: bob.toV2().getPublicKeyBundle(),
					context: InvitationV1.Context.with {
						$0.conversationID = conversationID
					})
		}

		// Repeatedly making the same invite should use the same topic/keys
		let original = try makeInvite("example.com/conversation-foo");
		for i in 1...10 {
			let invite = try makeInvite("example.com/conversation-foo");
			XCTAssertEqual(original.topic, invite.topic);
		}

		// But when the conversationId changes then it use a new topic/keys
		let invite = try makeInvite("example.com/conversation-bar");
		XCTAssertNotEqual(original.topic, invite.topic);
	}

	func testGenerateSealedInvitation() async throws {
		let aliceWallet = try FakeWallet.generate()
		let bobWallet = try FakeWallet.generate()

		let alice = try await PrivateKeyBundleV1.generate(wallet: aliceWallet)
		let bob = try await PrivateKeyBundleV1.generate(wallet: bobWallet)

		let invitation = try InvitationV1.createDeterministic(
			sender: alice.toV2(),
			recipient: bob.toV2().getPublicKeyBundle()
		)

		let newInvitation = try SealedInvitation.createV1(
			sender: try alice.toV2(),
			recipient: try bob.toV2().getPublicKeyBundle(),
			created: Date(),
			invitation: invitation
		)

		let deserialized = try SealedInvitation(serializedData: try newInvitation.serializedData())

		XCTAssert(!deserialized.v1.headerBytes.isEmpty, "header bytes empty")
		XCTAssertEqual(newInvitation, deserialized)

		let header = newInvitation.v1.header

		// Ensure the headers haven't been mangled
		XCTAssertEqual(header.sender, try alice.toV2().getPublicKeyBundle())
		XCTAssertEqual(header.recipient, try bob.toV2().getPublicKeyBundle())

		// Ensure alice can decrypt the invitation
		let aliceInvite = try newInvitation.v1.getInvitation(viewer: try alice.toV2())
		XCTAssertEqual(aliceInvite.topic, invitation.topic)
		XCTAssertEqual(aliceInvite.aes256GcmHkdfSha256.keyMaterial, invitation.aes256GcmHkdfSha256.keyMaterial)

		// Ensure bob can decrypt the invitation
		let bobInvite = try newInvitation.v1.getInvitation(viewer: try bob.toV2())
		XCTAssertEqual(bobInvite.topic, invitation.topic)
		XCTAssertEqual(bobInvite.aes256GcmHkdfSha256.keyMaterial, invitation.aes256GcmHkdfSha256.keyMaterial)
	}
}
