//
//  MessageTests.swift
//
//
//  Created by Pat Nakajima on 11/27/22.
//

import XCTest
@testable import XMTP

@available(iOS 16.0, *)
class MessageTests: XCTestCase {
	func testFullyEncodesDecodesMessagesV1() async throws {
		for _ in 0 ... 10 {
			let aliceWallet = try PrivateKey.generate()
			let bobWallet = try PrivateKey.generate()

			let alice = try await PrivateKeyBundleV1.generate(wallet: aliceWallet)
			let bob = try await PrivateKeyBundleV1.generate(wallet: bobWallet)

			let content = Data("Yo!".utf8)
			let message1 = try MessageV1.encode(
				sender: alice,
				recipient: bob.toPublicKeyBundle(),
				message: content,
				timestamp: Date()
			)

			XCTAssertEqual(aliceWallet.walletAddress, message1.senderAddress)
			XCTAssertEqual(bobWallet.walletAddress, message1.recipientAddress)

			let decrypted = try message1.decrypt(with: alice)
			XCTAssertEqual(decrypted, content)

//			let message2 = try MessageV1(serializedData: try message1.serializedData())
//			let message2Decrypted = try message2.decrypt(with: alice)
//			XCTAssertEqual(message2.senderAddress, aliceWallet.walletAddress)
//			XCTAssertEqual(message2.recipientAddress, bobWallet.walletAddress)
//			XCTAssertEqual(message2Decrypted, content)
		}
	}

	func testFullyEncodesDecodesMessagesV2() async throws {
		let aliceWallet = try PrivateKey.generate()
		let bobWallet = try PrivateKey.generate()

		let alice = try await PrivateKeyBundleV1.generate(wallet: aliceWallet)
		let bob = try await PrivateKeyBundleV1.generate(wallet: bobWallet)

		let client = try await Client.create(account: aliceWallet)
		var invitationContext = InvitationV1.Context()
		invitationContext.conversationID = "https://example.com/1"

		let invitationv1 = try InvitationV1.createRandom(context: invitationContext)
		let content = Data("Yo!".utf8)

		let sealedInvitation = try SealedInvitation.createV1(sender: alice.toV2(), recipient: bob.toV2().getPublicKeyBundle(), created: Date(), invitation: invitationv1)
		let conversation = try ConversationV2.create(client: client, invitation: invitationv1, header: sealedInvitation.v1.header)
		let message1 = try await MessageV2.encode(client: client, content: "Yo!", topic: invitationv1.topic, keyMaterial: invitationv1.aes256GcmHkdfSha256.keyMaterial)

		let decoded = try MessageV2.decode(message1, keyMaterial: invitationv1.aes256GcmHkdfSha256.keyMaterial)
		XCTAssertEqual(decoded.body, "Yo!")
	}

	func testCanDecrypt() throws {
		// All of these values were generated from xmtp-js
		let content = "0a120a08786d74702e6f7267120474657874180112110a08656e636f64696e6712055554462d3822026869".web3.bytesFromHex!
		let salt = "48c6c40ce9998a8684937b2bd90c492cef66c9cd92b4a30a4f811b43fd0aed79".web3.bytesFromHex!
		let nonce = "31f78d2c989a37d8471a5d40".web3.bytesFromHex!
		let secret = "04c86317929a0c223f44827dcf1290012b5e6538a54282beac85c2b16062fc8f781b52bea90e8c7c028254c6ba57ac144a56f054d569c340e73c6ff37aee4e68fc04a0fdb4e9c404f5d246a9fe2308f950f8374b0696dd98cc1c97fcbdbc54383ac862abee69c107723e1aa809cfbc587253b943476dc89c126af4f6515161a826ca04801742d6c45ee150a28f80cbcffd78a0210fe73ffdd74e4af8fd6307fb3d622d873653ca4bd47deb4711ef02611e5d64b4bcefcc481e236979af2b6156863e68".web3.bytesFromHex!
		let payload = "d752fb09ee0390fe5902a1bd7b2f530da7e5b3a2bd91bad9df8fa284ab63327b86a59620fd3e2d2cf9183f46bd0fe75bda3caca893420c38416b1f".web3.bytesFromHex!
		let additionalData = "0aac020a940108d995eeadcc3012460a440a408f20c9fc03909edeb21538b0a568c423f8829e95c0270779ca704f72a45f02416f6071f6faaf421cac3bacc6bb432fc4b5f92bc4391349953c7c98f12253cdd710011a430a4104b7eb7b56059a4f08bf3dd8f1b329e21d486e39822f17db15bad0d7f689f6c8081ae2800b9014fc9ef355a39e10503fddfdfa0b07ccc1946c2275b10e660d5ded12920108e995eeadcc3012440a420a40da669aa014468ffe34d5b962443d8b1e353b1e39f252bbcffa5c6c70adf9f7d2484de944213f345bac869e8c1942657b9c59f6fc12d139171b22789bc76ffb971a430a4104901d3a7f728bde1f871bcf46d44dcf34eead4c532135913583268d35bd93ca0a1571a8cb6546ab333f2d77c3bb9839be7e8f27795ea4d6e979b6670dec20636d12aa020a920108bad3eaadcc3012440a420a4016d83a6e44ee8b9764f18fbb390f2a4049d92ff904ebd75c76a71d58a7f943744f8bed7d3696f9fb41ce450c5ab9f4a7f9a83e3d10f401bbe85e3992c5156d491a430a41047cebe3a23e573672363665d13220d368d37776e10232de9bd382d5af36392956dbd806f8b78bec5cdc111763e4ef4aff7dee65a8a15fee8d338c387320c5b23912920108bad3eaadcc3012440a420a404a751f28001f34a4136529a99e738279856da6b32a1ee9dba20849d9cd84b6165166a6abeae1139ed8df8be3b4594d9701309075f2b8d5d4de1f713fb62ae37e1a430a41049c45e552ac9f69c083bd358acac31a2e3cf7d9aa9298fef11b43252730949a39c68272302a61b548b13452e19272c119b5189a5d7b5c3283a37d5d9db5ed0c6818b286deaecc30".web3.bytesFromHex!

		var ciphertext = CipherText()
		ciphertext.aes256GcmHkdfSha256.gcmNonce = Data(nonce)
		ciphertext.aes256GcmHkdfSha256.hkdfSalt = Data(salt)
		ciphertext.aes256GcmHkdfSha256.payload = Data(payload)

		let decrypted = try Crypto.decrypt(Data(secret), ciphertext, additionalData: Data(additionalData))

		XCTAssertEqual(Data(content), decrypted)

		let message = try EncodedContent(serializedData: decrypted)
		print(try message.textFormatString())
	}
}
