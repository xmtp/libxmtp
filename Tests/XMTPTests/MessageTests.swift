//
//  MessageTests.swift
//
//
//  Created by Pat Nakajima on 11/27/22.
//

import CryptoKit
import XCTest
import XMTPTestHelpers
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
		let sealedInvitation = try SealedInvitation.createV1(sender: alice.toV2(), recipient: bob.toV2().getPublicKeyBundle(), created: Date(), invitation: invitationv1)
		let encoder = TextCodec()
		let encodedContent = try encoder.encode(content: "Yo!")
		let message1 = try await MessageV2.encode(client: client, content: encodedContent, topic: invitationv1.topic, keyMaterial: invitationv1.aes256GcmHkdfSha256.keyMaterial)

		let decoded = try MessageV2.decode(message1, keyMaterial: invitationv1.aes256GcmHkdfSha256.keyMaterial)
		let result: String = try decoded.content()
		XCTAssertEqual(result, "Yo!")
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
	}

	func testGetsV1ID() async throws {
		let fixtures = await fixtures()
		let conversation = try await fixtures.aliceClient.conversations.newConversation(with: fixtures.bob.walletAddress)

		try await conversation.send(text: "hi")
		let envelope = fixtures.fakeApiClient.published.last!
		let decodedMessage = try conversation.decode(envelope)

		XCTAssertEqual(Data(SHA256.hash(data: envelope.message).bytes).toHex, decodedMessage.id)
	}

	func testGetsV2ID() async throws {
		let envelopeMessageData = Data(
			"12bf040a470880dedf9dafc0ff9e17123b2f786d74702f302f6d2d32536b644e355161305a6d694649357433524662667749532d4f4c76356a7573716e6465656e544c764e672f70726f746f12f3030af0030a20439174a205643a50af33c7670341338526dbb9c1cf0560687ff8a742e957282d120c090ba2b385b40639867493ce1abd037648c947f72e5c62e8691d7748e78f9a346ff401c97a628ebecf627d722829ff9cfb7d7c3e0b9e26b5801f2b5a39fd58757cc5771427bfefad6243f52cfc84b384fa042873ebeb90948aa80ca34f26ff883d64720c9228ed6bcd1a5c46953a12ae8732fd70260651455674e2e2c23bc8d64ed35562fef4cdfc55d38e72ad9cf2d597e68f48b6909967b0f5d0b4f33c0af3efce55c739fbc93888d20b833df15811823970a356b26622936564d830434d3ecde9a013f7433142e366f1df5589131e440251be54d5d6deef9aaaa9facac26eb54fb7b74eb48c5a2a9a2e2956633b123cc5b91dec03e4dba30683be03bd7510f16103d3f81712dccf2be003f2f77f9e1f162bc47f6c1c38a1068abd3403952bef31d75e8024e7a62d9a8cbd48f1872a0156abb559d01de689b4370a28454658957061c46f47fc5594808d15753876d4b5408b3a3410d0555c016e427dfceae9c05a4a21fd7ce4cfbb11b2a696170443cf310e0083b0a48e357fc2f00c688c0b56821c8a14c2bb44ddfa31d680dfc85efe4811e86c6aa3adfc373ad5731ddab83960774d98d60075b8fd70228da5d748bfb7a5334bd07e1cc4a9fbf3d5de50860d0684bb27786b5b4e00d415".web3.bytesFromHex!
		)

		let envelope = try Envelope.with { envelope in
			envelope.contentTopic = "/xmtp/0/m-2SkdN5Qa0ZmiFI5t3RFbfwIS-OLv5jusqndeenTLvNg/proto"
			envelope.message = envelopeMessageData
			envelope.timestampNs = UInt64(Date().millisecondsSinceEpoch)
		}

		let key = try PrivateKey.with { key in
			key.secp256K1.bytes = Data([
				80, 84, 15, 126, 14, 105, 216, 8,
				61, 147, 153, 232, 103, 69, 219, 13,
				99, 118, 68, 56, 160, 94, 58, 22,
				140, 247, 221, 172, 14, 188, 52, 88,
			])

			key.publicKey.secp256K1Uncompressed.bytes = try KeyUtil.generatePublicKey(from: key.secp256K1.bytes)
		}

		let keyBundleData = Data(
			"0a86030ac001089387b882df3012220a204a393d6ac64c10770a2585def70329f10ca480517311f0b321a5cfbbae0119951a9201089387b882df3012440a420a4092f66532cf0266d146a17060fb64148e4a6adc673c14511e45f40ac66551234a336a8feb6ef3fabdf32ea259c2a3bca32b9550c3d34e004ea59e86b42f8001ac1a430a41041c919edda3399ab7f20f5e1a9339b1c2e666e80a164fb1c6d8bc1b7dbf2be158f87c837a6364c7fb667a40c2d234d198a7c2168a928d39409ad7d35d653d319912c00108a087b882df3012220a202ade2eefefa5f8855e557d685278e8717e3f57682b66c3d73aa87896766acddc1a920108a087b882df3012440a420a404f4a90ef10e1536e4588f12c2320229008d870d2abaecd1acfefe9ca91eb6f6d56b1380b1bdebdcf9c46fb19ceb3247d5d986a4dd2bce40a4bdf694c24b08fbb1a430a4104a51efe7833c46d2f683e2eb1c07811bb96ab5e4c2000a6f06124968e8842ff8be737ad7ca92b2dabb13550cdc561df15771c8494eca7b7ca5519f6da02f76489".web3.bytesFromHex!
		)
		let keyBundle = try PrivateKeyBundle(serializedData: keyBundleData)

		let client = try await Client.from(bundle: keyBundle)

		let conversationJSON = Data("""
		{"version":"v2","topic":"/xmtp/0/m-2SkdN5Qa0ZmiFI5t3RFbfwIS-OLv5jusqndeenTLvNg/proto","keyMaterial":"ATA1L0O2aTxHmskmlGKCudqfGqwA1H+bad3W/GpGOr8=","peerAddress":"0x436D906d1339fC4E951769b1699051f020373D04","createdAt":"2023-01-26T22:58:45.068Z","context":{"conversationId":"pat/messageid","metadata":{}}}
		""".utf8)

		let decoder = JSONDecoder()
		guard case let .v2(decodedConversation) = try client.importConversation(from: conversationJSON) else {
			XCTFail("did not get v2 conversation")
			return
		}

		let conversation = ConversationV2(topic: decodedConversation.topic, keyMaterial: decodedConversation.keyMaterial, context: InvitationV1.Context(), peerAddress: decodedConversation.peerAddress, client: client, header: SealedInvitationHeaderV1())

		let decodedMessage = try conversation.decode(envelope: envelope)
		XCTAssertEqual(decodedMessage.id, "e42a7dd44d0e1214824eab093cb89cfe6f666298d0af2d54fe0c914c8b72eff3")
	}
}
