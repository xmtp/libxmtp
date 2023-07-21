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

    func testGeneratesKnownDeterministicTopic() async throws {
        // address = 0xF56d1F3b1290204441Cb3843C2Cac1C2f5AEd690
        let aliceKeyData = Data(("0x0a8a030ac20108c192a3f7923112220a2068d2eb2ef8c50c4916b42ce638c5610e44ff4eb3ecb098" +
                                "c9dacf032625c72f101a940108c192a3f7923112460a440a40fc9822283078c323c9319c45e60ab4" +
                                "2c65f6e1744ed8c23c52728d456d33422824c98d307e8b1c86a26826578523ba15fe6f04a17fca17" +
                                "6664ee8017ec8ba59310011a430a410498dc2315dd45d99f5e900a071e7b56142de344540f07fbc7" +
                                "3a0f9a5d5df6b52eb85db06a3825988ab5e04746bc221fcdf5310a44d9523009546d4bfbfbb89cfb" +
                                "12c20108eb92a3f7923112220a20788be9da8e1a1a08b05f7cbf22d86980bc056b130c482fa5bd26" +
                                "ccb8d29b30451a940108eb92a3f7923112460a440a40a7afa25cb6f3fbb98f9e5cd92a1df1898452" +
                                "e0dfa1d7e5affe9eaf9b72dd14bc546d86c399768badf983f07fa7dd16eee8d793357ce6fccd6768" +
                                "07d87bcc595510011a430a410422931e6295c3c93a5f6f5e729dc02e1754e916cb9be16d36dc163a" +
                                "300931f42a0cd5fde957d75c2068e1980c5f86843daf16aba8ae57e8160b8b9f0191def09e").web3.bytesFromHex!)
        let aliceKeys = try PrivateKeyBundle(serializedData: aliceKeyData).v1.toV2()

        // address = 0x3De402A325323Bb97f00cE3ad5bFAc96A11F9A34
        let bobKeyData = Data(("0x0a88030ac001088cd68df7923112220a209057f8d813314a2aae74e6c4c30f909c1c496b6037ce32" +
                               "a12c613558a8e961681a9201088cd68df7923112440a420a40501ae9b4f75d5bb5bae3ca4ecfda4e" +
                               "de9edc5a9b7fc2d56dc7325b837957c23235cc3005b46bb9ef485f106404dcf71247097ed5096355" +
                               "90f4b7987b833d03661a430a4104e61a7ae511567f4a2b5551221024b6932d6cdb8ecf3876ec64cf" +
                               "29be4291dd5428fc0301963cdf6939978846e2c35fd38fcb70c64296a929f166ef6e4e91045712c2" +
                               "0108b8d68df7923112220a2027707399474d417bf6aae4baa3d73b285bf728353bc3e156b0e32461" +
                               "ebb48f8c1a940108b8d68df7923112460a440a40fb96fa38c3f013830abb61cf6b39776e0475eb13" +
                               "79c66013569c3d2daecdd48c7fbee945dcdbdc5717d1f4ffd342c4d3f1b7215912829751a94e3ae1" +
                               "1007e0a110011a430a4104952b7158cfe819d92743a4132e2e3ae867d72f6a08292aebf471d0a7a2" +
                               "907f3e9947719033e20edc9ca9665874bd88c64c6b62c01928065f6069c5c80c699924").web3.bytesFromHex!)
        let bobKeys = try PrivateKeyBundle(serializedData: bobKeyData)
        
        let invite = try InvitationV1.createDeterministic(sender: aliceKeys, recipient: bobKeys.v1.toV2().getPublicKeyBundle(), context: InvitationV1.Context.with { $0.conversationID = "test" })

        XCTAssertEqual(invite.topic, "/xmtp/0/m-4b52be1e8567d72d0bc407debe2d3c7fca2ae93a47e58c3f9b5c5068aff80ec5/proto")
    }
}
