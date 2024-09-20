//
//  ConversationTests.swift
//
//
//  Created by Pat Nakajima on 12/6/22.
//

import CryptoKit
import XCTest
@testable import XMTPiOS
import XMTPTestHelpers

@available(iOS 16, *)
class ConversationTests: XCTestCase {
	var alice: PrivateKey!
	var aliceClient: Client!

	var bob: PrivateKey!
	var bobClient: Client!

	var fixtures: Fixtures!

	override func setUp() async throws {
		fixtures = await fixtures()

		alice = fixtures.alice
		bob = fixtures.bob

		aliceClient = fixtures.aliceClient
		bobClient = fixtures.bobClient
	}

	func testCanPrepareV2Message() async throws {
		let conversation = try await aliceClient.conversations.newConversation(with: bob.address)
		let preparedMessage = try await conversation.prepareMessage(content: "hi")
		let messageID = preparedMessage.messageID

		try await conversation.send(prepared: preparedMessage)

		let messages = try await conversation.messages()
		let message = messages[0]

		XCTAssertEqual("hi", message.body)
		XCTAssertEqual(message.id, messageID)
	}

	func testCanSendPreparedMessagesWithoutAConversation() async throws {
		let conversation = try await aliceClient.conversations.newConversation(with: bob.address)
		let preparedMessage = try await conversation.prepareMessage(content: "hi")
		let messageID = preparedMessage.messageID

		// This does not need the `conversation` to `.publish` the message.
		// This simulates a background task publishes all pending messages upon connection.
		try await aliceClient.publish(envelopes: preparedMessage.envelopes)

		let messages = try await conversation.messages()
		let message = messages[0]

		XCTAssertEqual("hi", message.body)
		XCTAssertEqual(message.id, messageID)
	}

	func testV2RejectsSpoofedContactBundles() async throws {
		let topic =
			"/xmtp/0/m-Gdb7oj5nNdfZ3MJFLAcS4WTABgr6al1hePy6JV1-QUE/proto"
		guard let envelopeMessage = Data(base64String: "Er0ECkcIwNruhKLgkKUXEjsveG10cC8wL20tR2RiN29qNW5OZGZaM01KRkxBY1M0V1RBQmdyNmFsMWhlUHk2SlYxLVFVRS9wcm90bxLxAwruAwognstLoG6LWgiBRsWuBOt+tYNJz+CqCj9zq6hYymLoak8SDFsVSy+cVAII0/r3sxq7A/GCOrVtKH6J+4ggfUuI5lDkFPJ8G5DHlysCfRyFMcQDIG/2SFUqSILAlpTNbeTC9eSI2hUjcnlpH9+ncFcBu8StGfmilVGfiADru2fGdThiQ+VYturqLIJQXCHO2DkvbbUOg9xI66E4Hj41R9vE8yRGeZ/eRGRLRm06HftwSQgzAYf2AukbvjNx/k+xCMqti49Qtv9AjzxVnwttLiA/9O+GDcOsiB1RQzbZZzaDjQ/nLDTF6K4vKI4rS9QwzTJqnoCdp0SbMZFf+KVZpq3VWnMGkMxLW5Fr6gMvKny1e1LAtUJSIclI/1xPXu5nsKd4IyzGb2ZQFXFQ/BVL9Z4CeOZTsjZLGTOGS75xzzGHDtKohcl79+0lgIhAuSWSLDa2+o2OYT0fAjChp+qqxXcisAyrD5FB6c9spXKfoDZsqMV/bnCg3+udIuNtk7zBk7jdTDMkofEtE3hyIm8d3ycmxKYOakDPqeo+Nk1hQ0ogxI8Z7cEoS2ovi9+rGBMwREzltUkTVR3BKvgV2EOADxxTWo7y8WRwWxQ+O6mYPACsiFNqjX5Nvah5lRjihphQldJfyVOG8Rgf4UwkFxmI"),
					let keyMaterial = Data(base64String: "R0BBM5OPftNEuavH/991IKyJ1UqsgdEG4SrdxlIG2ZY=")
		else {
			XCTFail("did not have correct setup data")
			return
		}

		let conversation = ConversationV2(topic: topic, keyMaterial: keyMaterial, context: .init(), peerAddress: "0x2f25e33D7146602Ec08D43c1D6B1b65fc151A677", client: aliceClient)

		let envelope = Envelope(topic: topic, timestamp: Date(), message: envelopeMessage)
		XCTAssertThrowsError(try conversation.decode(envelope: envelope)) { error in
			switch error as! MessageV2Error {
			case let .decodeError(message):
				XCTAssertEqual(message, "pre-key not signed by identity key")
			default:
				XCTFail("did not raise correct error")
			}
		}
	}

	func testDoesNotAllowConversationWithSelf() async throws {
		let expectation = XCTestExpectation(description: "convo with self throws")
		let client = aliceClient!

		do {
			_ = try await client.conversations.newConversation(with: alice.walletAddress)
		} catch {
			expectation.fulfill()
		}

		await fulfillment(of: [expectation], timeout: 3)
	}

	func testCanStreamConversationsV2() async throws {
		let options = ClientOptions(api: ClientOptions.Api(env: .local, isSecure: false))
		let wallet = try PrivateKey.generate()
		let client = try await Client.create(account: wallet, options: options)
		
		let wallet2 = try PrivateKey.generate()
		let client2 = try await Client.create(account: wallet2, options: options)
		let expectation1 = XCTestExpectation(description: "got a conversation")
		expectation1.expectedFulfillmentCount = 2

		Task(priority: .userInitiated) {
			for try await conversation in try await client.conversations.stream() {
				expectation1.fulfill()
			}
		}

		guard case let .v2(conversation) = try await client.conversations.newConversation(with: client2.address) else {
			XCTFail("Did not create a v2 convo")
			return
		}
		try? await Task.sleep(nanoseconds: 1_000_000_000)

		try await conversation.send(content: "hi")

		guard case let .v2(conversation) = try await client.conversations.newConversation(with: client2.address) else {
			XCTFail("Did not create a v2 convo")
			return
		}
		
		try? await Task.sleep(nanoseconds: 15_000_000_000)

		try await conversation.send(content: "hi again")

		let newWallet = try PrivateKey.generate()
		let newClient = try await Client.create(account: newWallet, options: options)

		guard case let .v2(conversation2) = try await client.conversations.newConversation(with: newWallet.walletAddress) else {
			XCTFail("Did not create a v2 convo")
			return
		}
		try? await Task.sleep(nanoseconds: 1_000_000_000)

		try await conversation2.send(content: "hi from new wallet")

		await fulfillment(of: [expectation1], timeout: 30)
	}

	func publishLegacyContact(client: Client) async throws {
		var contactBundle = ContactBundle()
		contactBundle.v1.keyBundle = try client.v1keys.toPublicKeyBundle()

		var envelope = Envelope()
		envelope.contentTopic = Topic.contact(client.address).description
		envelope.timestampNs = UInt64(Date().millisecondsSinceEpoch * 1_000_000)
		envelope.message = try contactBundle.serializedData()

		try await client.publish(envelopes: [envelope])
	}

	func testStreamingMessagesFromV2Conversations() async throws {
		guard case let .v2(conversation) = try await aliceClient.conversations.newConversation(with: bob.walletAddress) else {
			XCTFail("Did not get a v2 convo")
			return
		}

		let expectation = XCTestExpectation(description: "got a message")

		Task(priority: .userInitiated) {
			for try await message in conversation.streamMessages() {
				if message.body == "hi alice" {
					expectation.fulfill()
				}
			}
		}

		// Stream a message
		try await conversation.send(content: "hi alice")

		await fulfillment(of: [expectation], timeout: 3)
	}

	func testCanLoadV2Messages() async throws {
		guard case let .v2(bobConversation) = try await bobClient.conversations.newConversation(with: alice.address, context: InvitationV1.Context(conversationID: "hi")) else {
			XCTFail("did not get a v2 conversation for alice")
			return
		}

		guard case let .v2(aliceConversation) = try await aliceClient.conversations.newConversation(with: bob.address, context: InvitationV1.Context(conversationID: "hi")) else {
			XCTFail("did not get a v2 conversation for alice")
			return
		}

		try await bobConversation.send(content: "hey alice")
		let messages = try await aliceConversation.messages()

		XCTAssertEqual(1, messages.count)
		XCTAssertEqual("hey alice", messages[0].body)
		XCTAssertEqual(bob.address, messages[0].senderAddress)
	}

	func testVerifiesV2MessageSignature() async throws {
		guard case let .v2(aliceConversation) = try await aliceClient.conversations.newConversation(with: bob.address, context: InvitationV1.Context(conversationID: "hi")) else {
			XCTFail("did not get a v2 conversation for alice")
			return
		}

		let codec = TextCodec()
		let originalContentText = "hello"
		let originalContent = try codec.encode(content: originalContentText, client: aliceClient)
		let tamperedContent = try codec.encode(content: "this is a fake", client: aliceClient)

		let originalPayload = try originalContent.serializedData()
		let tamperedPayload = try tamperedContent.serializedData()

		let date = Date()
		let header = MessageHeaderV2(topic: aliceConversation.topic, created: date)
		let headerBytes = try header.serializedData()

		let digest = SHA256.hash(data: headerBytes + tamperedPayload)
		let preKey = try aliceClient.keys.preKeys[0]
		let signature = try await preKey.sign(Data(digest))

		let bundle = try aliceClient.v1keys.toV2().getPublicKeyBundle()

		let signedContent = SignedContent(payload: originalPayload, sender: bundle, signature: signature)
		let signedBytes = try signedContent.serializedData()

		let ciphertext = try Crypto.encrypt(aliceConversation.keyMaterial, signedBytes, additionalData: headerBytes)
		
		let thirtyDayPeriodsSinceEpoch = Int(date.timeIntervalSince1970 / 60 / 60 / 24 / 30)
		let info = "\(thirtyDayPeriodsSinceEpoch)-\(aliceClient.address)"
		let infoEncoded = info.data(using: .utf8)
		
		let senderHmac = try Crypto.generateHmacSignature(secret: aliceConversation.keyMaterial, info: infoEncoded!, message: headerBytes)
		
		let shouldPush = try codec.shouldPush(content: originalContentText)

		let tamperedMessage = MessageV2(
			headerBytes: headerBytes,
			ciphertext: ciphertext,
			senderHmac: senderHmac,
			shouldPush: shouldPush
		)

		try await aliceClient.publish(envelopes: [
			Envelope(topic: aliceConversation.topic, timestamp: Date(), message: Message(v2: tamperedMessage).serializedData()),
		])

		guard case let .v2(bobConversation) = try await bobClient.conversations.newConversation(with: alice.address, context: InvitationV1.Context(conversationID: "hi")) else {
			XCTFail("did not get a v2 conversation for alice")
			return
		}

		let messages = try await bobConversation.messages()
		XCTAssertEqual(0, messages.count, "did not filter out tampered message")
	}

	func testCanPaginateV1Messages() async throws {
		throw XCTSkip("this test is flakey in CI, TODO: figure it out")
		// Overwrite contact as legacy so we can get v1
		try await publishLegacyContact(client: bobClient)
		try await publishLegacyContact(client: aliceClient)

		guard case let .v1(bobConversation) = try await bobClient.conversations.newConversation(with: alice.address) else {
			XCTFail("did not get a v1 conversation for alice")
			return
		}

		guard case let .v1(aliceConversation) = try await aliceClient.conversations.newConversation(with: bob.address) else {
			XCTFail("did not get a v1 conversation for alice")
			return
		}

		// This is just to verify that the fake API client can handle limits larger how many envelopes it knows about
		_ = try await aliceConversation.messages(limit: -1)

		try await bobConversation.send(content: "hey alice 1", sentAt: Date().addingTimeInterval(-1000))
		try await bobConversation.send(content: "hey alice 2", sentAt: Date().addingTimeInterval(-500))
		try await bobConversation.send(content: "hey alice 3", sentAt: Date())

		let messages = try await aliceConversation.messages(limit: 1)
		XCTAssertEqual(1, messages.count)
		XCTAssertEqual("hey alice 3", messages[0].body)
		XCTAssertEqual(aliceConversation.topic.description, messages[0].topic)

		let messages2 = try await aliceConversation.messages(limit: 1, before: messages[0].sent)
		XCTAssertEqual(1, messages2.count)
		XCTAssertEqual("hey alice 2", messages2[0].body)

		// This is just to verify that the fake API client can handle limits larger how many envelopes it knows about
		_ = try await aliceConversation.messages(limit: 10)
	}

	func testCanPaginateV2Messages() async throws {
		guard case let .v2(bobConversation) = try await bobClient.conversations.newConversation(with: alice.address, context: InvitationV1.Context(conversationID: "hi")) else {
			XCTFail("did not get a v2 conversation for alice")
			return
		}

		guard case let .v2(aliceConversation) = try await aliceClient.conversations.newConversation(with: bob.address, context: InvitationV1.Context(conversationID: "hi")) else {
			XCTFail("did not get a v2 conversation for alice")
			return
		}

		try await bobConversation.send(content: "hey alice 1", sentAt: Date().addingTimeInterval(-1000))
		try await bobConversation.send(content: "hey alice 2", sentAt: Date().addingTimeInterval(-500))
		try await bobConversation.send(content: "hey alice 3", sentAt: Date())

		let messages = try await aliceConversation.messages(limit: 1)
		XCTAssertEqual(1, messages.count)
		XCTAssertEqual("hey alice 3", messages[0].body)
		XCTAssertEqual(aliceConversation.topic, messages[0].topic)

		let messages2 = try await aliceConversation.messages(limit: 1, before: messages[0].sent)
		XCTAssertEqual(1, messages2.count)
		XCTAssertEqual("hey alice 2", messages2[0].body)
	}

	func testCanRetrieveAllMessages() async throws {
		guard case let .v2(bobConversation) = try await bobClient.conversations.newConversation(with: alice.address, context: InvitationV1.Context(conversationID: "hi")) else {
			XCTFail("did not get a v2 conversation for bob")
			return
		}

		guard case let .v2(aliceConversation) = try await aliceClient.conversations.newConversation(with: bob.address, context: InvitationV1.Context(conversationID: "hi")) else {
			XCTFail("did not get a v2 conversation for alice")
			return
		}

		for i in 0 ..< 110 {
			do {
				let content = "hey alice \(i)"
				let sentAt = Date().addingTimeInterval(-1000)
				try await bobConversation.send(content: content, sentAt: sentAt)
			} catch {
				print("Error sending message:", error)
			}
		}

		let messages = try await aliceConversation.messages()
		XCTAssertEqual(110, messages.count)
	}

	func testCanRetrieveBatchMessages() async throws {
		guard case let .v2(bobConversation) = try await aliceClient.conversations.newConversation(with: bob.address, context: InvitationV1.Context(conversationID: "hi")) else {
			XCTFail("did not get a v2 conversation for bob")
			return
		}

		for i in 0 ..< 3 {
			do {
				let content = "hey alice \(i)"
				let sentAt = Date().addingTimeInterval(-1000)
				try await bobConversation.send(content: content, sentAt: sentAt)
			} catch {
				print("Error sending message:", error)
			}
		}

		let messages = try await aliceClient.conversations.listBatchMessages(
			topics: [bobConversation.topic: Pagination(limit: 3)]
		)
		XCTAssertEqual(3, messages.count)
		XCTAssertEqual(bobConversation.topic, messages[0].topic)
		XCTAssertEqual(bobConversation.topic, messages[1].topic)
		XCTAssertEqual(bobConversation.topic, messages[2].topic)
	}

	func testProperlyDiscardBadBatchMessages() async throws {
		guard case let .v2(bobConversation) = try await aliceClient.conversations
			.newConversation(with: bob.address)
		else {
			XCTFail("did not get a v2 conversation for bob")
			return
		}

		try await bobConversation.send(content: "Hello")

		// Now we send some garbage and expect it to be properly ignored.
		try await bobClient.apiClient!.publish(envelopes: [
			Envelope(
				topic: bobConversation.topic,
				timestamp: Date(),
				message: Data([1, 2, 3]) // garbage, malformed message
			),
		])

		try await bobConversation.send(content: "Goodbye")

		let messages = try await aliceClient.conversations.listBatchMessages(
			topics: [bobConversation.topic: nil]
		)
		XCTAssertEqual(2, messages.count)
		XCTAssertEqual("Goodbye", try messages[0].content())
		XCTAssertEqual("Hello", try messages[1].content())
	}

	func testImportV1ConversationFromJS() async throws {
		let jsExportJSONData = Data("""
		{
				"version": "v1",
				"peerAddress": "0x5DAc8E2B64b8523C11AF3e5A2E087c2EA9003f14",
				"createdAt": "2022-09-20T09:32:50.329Z"
		}
		""".utf8)

		let conversation = try aliceClient.importConversation(from: jsExportJSONData)

		XCTAssertEqual(try conversation?.peerAddress, "0x5DAc8E2B64b8523C11AF3e5A2E087c2EA9003f14")
	}

	func testImportV2ConversationFromJS() async throws {
		let jsExportJSONData = Data("""
		{"version":"v2","topic":"/xmtp/0/m-2SkdN5Qa0ZmiFI5t3RFbfwIS-OLv5jusqndeenTLvNg/proto","keyMaterial":"ATA1L0O2aTxHmskmlGKCudqfGqwA1H+bad3W/GpGOr8=","peerAddress":"0x436D906d1339fC4E951769b1699051f020373D04","createdAt":"2023-01-26T22:58:45.068Z","context":{"conversationId":"pat/messageid","metadata":{}}}
		""".utf8)

		let conversation = try aliceClient.importConversation(from: jsExportJSONData)
		XCTAssertEqual(try conversation?.peerAddress, "0x436D906d1339fC4E951769b1699051f020373D04")
	}

	func testImportV2ConversationWithNoContextFromJS() async throws {
		let jsExportJSONData = Data("""
		{"version":"v2","topic":"/xmtp/0/m-2SkdN5Qa0ZmiFI5t3RFbfwIS-OLv5jusqndeenTLvNg/proto","keyMaterial":"ATA1L0O2aTxHmskmlGKCudqfGqwA1H+bad3W/GpGOr8=","peerAddress":"0x436D906d1339fC4E951769b1699051f020373D04","createdAt":"2023-01-26T22:58:45.068Z"}
		""".utf8)

		guard case let .v2(conversation) = try aliceClient.importConversation(from: jsExportJSONData) else {
			XCTFail("did not get a v2 conversation")
			return
		}

		XCTAssertEqual(conversation.peerAddress, "0x436D906d1339fC4E951769b1699051f020373D04")
	}

	func testCanSendEncodedContentV2Message() async throws {
		guard case let .v2(bobConversation) = try await bobClient.conversations.newConversation(with: alice.address, context: InvitationV1.Context(conversationID: "hi")) else {
			XCTFail("did not get a v1 conversation for alice")
			return
		}

		guard case let .v2(aliceConversation) = try await aliceClient.conversations.newConversation(with: bob.address, context: InvitationV1.Context(conversationID: "hi")) else {
			XCTFail("did not get a v1 conversation for alice")
			return
		}

		let encodedContent = try TextCodec().encode(content: "hi", client: aliceClient)

		try await bobConversation.send(encodedContent: encodedContent)

		let messages = try await aliceConversation.messages()

		XCTAssertEqual(1, messages.count)
		XCTAssertEqual("hi", try messages[0].content())
	}
    
	func testCanHaveConsentState() async throws {
		let bobConversation = try await bobClient.conversations.newConversation(with: alice.address, context: InvitationV1.Context(conversationID: "hi"))
		let isAllowed = (try await bobConversation.consentState()) == .allowed

		// Conversations you start should start as allowed
		XCTAssertTrue(isAllowed)
        
        try await bobClient.contacts.deny(addresses: [alice.address])
        _ = try await bobClient.contacts.refreshConsentList()

        let isDenied = (try await bobConversation.consentState()) == .denied

        XCTAssertTrue(isDenied)

		let aliceConversation = (try await aliceClient.conversations.list())[0]
		let isUnknown = (try await aliceConversation.consentState()) == .unknown

		// Conversations started with you should start as unknown
		XCTAssertTrue(isUnknown)

		try await aliceClient.contacts.allow(addresses: [bob.address])

		let isBobAllowed = (try await aliceConversation.consentState()) == .allowed
		XCTAssertTrue(isBobAllowed)
	}
    
    func testCanHaveImplicitConsentOnMessageSend() async throws {
        let bobConversation = try await bobClient.conversations.newConversation(with: alice.address, context: InvitationV1.Context(conversationID: "hi"))
        let isAllowed = (try await bobConversation.consentState()) == .allowed

        // Conversations you start should start as allowed
        XCTAssertTrue(isAllowed)


        let aliceConversation = (try await aliceClient.conversations.list())[0]
        let isUnknown = (try await aliceConversation.consentState()) == .unknown

        // Conversations started with you should start as unknown
        XCTAssertTrue(isUnknown)

        try await aliceConversation.send(content: "hey bob")
        _ = try await aliceClient.contacts.refreshConsentList()
        let isNowAllowed = (try await aliceConversation.consentState()) == .allowed

        // Conversations you send a message to get marked as allowed
        XCTAssertTrue(isNowAllowed)
    }
}
