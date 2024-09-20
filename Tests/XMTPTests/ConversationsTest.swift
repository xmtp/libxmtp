//
//  ConversationsTests.swift
//
//
//  Created by Pat on 2/16/23.
//

import Foundation
import XCTest
@testable import XMTPiOS
import XMTPTestHelpers
import CryptoKit

@available(macOS 13.0, *)
@available(iOS 16, *)
class ConversationsTests: XCTestCase {
	func testCanGetConversationFromIntroEnvelope() async throws {
		let fixtures = await fixtures()
		let client = fixtures.aliceClient!

		let created = Date()

		let message = try MessageV1.encode(
			sender: try fixtures.bobClient.v1keys,
			recipient: fixtures.aliceClient.v1keys.toPublicKeyBundle(),
			message: try TextCodec().encode(content: "hello", client: client).serializedData(),
			timestamp: created
		)

		let envelope = Envelope(topic: .userIntro(client.address), timestamp: created, message: try Message(v1: message).serializedData())

		let conversation = try await client.conversations.fromIntro(envelope: envelope)
		XCTAssertEqual(try conversation.peerAddress, fixtures.bob.address)
		XCTAssertEqual(conversation.createdAt.description, created.description)
	}

	func testCanGetConversationFromInviteEnvelope() async throws {
		let fixtures = await fixtures()
		let client: Client = fixtures.aliceClient!

		let created = Date()

		let invitation = try InvitationV1.createDeterministic(
				sender: fixtures.bobClient.keys,
				recipient: client.keys.getPublicKeyBundle())
		let sealed = try SealedInvitation.createV1(
			sender: fixtures.bobClient.keys,
			recipient: client.keys.getPublicKeyBundle(),
			created: created,
			invitation: invitation
		)

		let peerAddress = fixtures.alice.walletAddress
		let envelope = Envelope(topic: .userInvite(peerAddress), timestamp: created, message: try sealed.serializedData())

		let conversation = try await client.conversations.fromInvite(envelope: envelope)
		XCTAssertEqual(try conversation.peerAddress, fixtures.bob.address)
		XCTAssertEqual(conversation.createdAt.description, created.description)
	}

	func testStreamAllMessagesGetsMessageFromKnownConversation() async throws {
		let fixtures = await fixtures()
		let client = fixtures.aliceClient!
		let bobConversation = try await fixtures.bobClient.conversations.newConversation(with: client.address)

		let expectation1 = expectation(description: "got a message")

		Task(priority: .userInitiated) {
			for try await _ in try await client.conversations.streamAllMessages() {
				expectation1.fulfill()
			}
		}
		
		try await Task.sleep(for: .milliseconds(500))
		_ = try await bobConversation.send(text: "hi")

		await waitForExpectations(timeout: 3)
	}
    
    func testCanValidateTopicsInsideConversation() async throws {
        let validId = "sdfsadf095b97a9284dcd82b2274856ccac8a21de57bebe34e7f9eeb855fb21126d3b8f"
        
        // Creation of all known types of topics
        let privateStore = Topic.userPrivateStoreKeyBundle(validId).description
        let contact = Topic.contact(validId).description
        let userIntro = Topic.userIntro(validId).description
        let userInvite = Topic.userInvite(validId).description
        let directMessageV1 = Topic.directMessageV1(validId, "sd").description
        let directMessageV2 = Topic.directMessageV2(validId).description
        let preferenceList = Topic.preferenceList(validId).description
        
        // check if validation of topics accepts all types
        XCTAssertTrue(Topic.isValidTopic(topic: privateStore))
        XCTAssertTrue(Topic.isValidTopic(topic: contact))
        XCTAssertTrue(Topic.isValidTopic(topic: userIntro))
        XCTAssertTrue(Topic.isValidTopic(topic: userInvite))
        XCTAssertTrue(Topic.isValidTopic(topic: directMessageV1))
        XCTAssertTrue(Topic.isValidTopic(topic: directMessageV2))
        XCTAssertTrue(Topic.isValidTopic(topic: preferenceList))
    }
    
    func testCannotValidateTopicsInsideConversation() async throws {
        let invalidId = "��\\u0005�!\\u000b���5\\u00001\\u0007�蛨\\u001f\\u00172��.����K9K`�"
        
        // Creation of all known types of topics
        let privateStore = Topic.userPrivateStoreKeyBundle(invalidId).description
        let contact = Topic.contact(invalidId).description
        let userIntro = Topic.userIntro(invalidId).description
        let userInvite = Topic.userInvite(invalidId).description
        let directMessageV1 = Topic.directMessageV1(invalidId, "sd").description
        let directMessageV2 = Topic.directMessageV2(invalidId).description
        let preferenceList = Topic.preferenceList(invalidId).description
        
        // check if validation of topics declines all types
        XCTAssertFalse(Topic.isValidTopic(topic: privateStore))
        XCTAssertFalse(Topic.isValidTopic(topic: contact))
        XCTAssertFalse(Topic.isValidTopic(topic: userIntro))
        XCTAssertFalse(Topic.isValidTopic(topic: userInvite))
        XCTAssertFalse(Topic.isValidTopic(topic: directMessageV1))
        XCTAssertFalse(Topic.isValidTopic(topic: directMessageV2))
        XCTAssertFalse(Topic.isValidTopic(topic: preferenceList))
    }
	
	func testReturnsAllHMACKeys() async throws {
		let alix = try PrivateKey.generate()
		let opts = ClientOptions(api: ClientOptions.Api(env: .local, isSecure: false))
		let alixClient = try await Client.create(
			account: alix,
			options: opts
		)
		var conversations: [Conversation] = []
		for _ in 0..<5 {
			let account = try PrivateKey.generate()
			let client = try await Client.create(account: account, options: opts)
			do {
				let newConversation = try await alixClient.conversations.newConversation(
					with: client.address,
					context: InvitationV1.Context(conversationID: "hi")
				)
				conversations.append(newConversation)
			} catch {
				print("Error creating conversation: \(error)")
			}
		}
		
		let thirtyDayPeriodsSinceEpoch = Int(Date().timeIntervalSince1970) / (60 * 60 * 24 * 30)
		
		let hmacKeys = await alixClient.conversations.getHmacKeys()
		
		let topics = hmacKeys.hmacKeys.keys
		conversations.forEach { conversation in
			XCTAssertTrue(topics.contains(conversation.topic))
		}
		
		var topicHmacs: [String: Data] = [:]
		let headerBytes = try Crypto.secureRandomBytes(count: 10)
		
		for conversation in conversations {
			let topic = conversation.topic
			let payload = try? TextCodec().encode(content: "Hello, world!", client: alixClient)
			
			_ = try await MessageV2.encode(
				client: alixClient,
				content: payload!,
				topic: topic,
				keyMaterial: headerBytes,
				codec: TextCodec()
			)
			
			let keyMaterial = conversation.keyMaterial
			let info = "\(thirtyDayPeriodsSinceEpoch)-\(alixClient.address)"
			let key = try Crypto.deriveKey(secret: keyMaterial!, nonce: Data(), info: Data(info.utf8))
			let hmac = try Crypto.calculateMac(headerBytes, key)
			
			topicHmacs[topic] = hmac
		}
		
		for (topic, hmacData) in hmacKeys.hmacKeys {
			for (idx, hmacKeyThirtyDayPeriod) in hmacData.values.enumerated() {
				let valid = Crypto.verifyHmacSignature(
					key: SymmetricKey(data: hmacKeyThirtyDayPeriod.hmacKey),
					signature: topicHmacs[topic]!,
					message: headerBytes
				)

				XCTAssertTrue(valid == (idx == 1))
			}
		}
	}
    
    func testSendConversationWithConsentSignature() async throws {
        let fixtures = await fixtures()

        let timestamp = UInt64(Date().timeIntervalSince1970 * 1000)
        let signatureText = Signature.consentProofText(peerAddress: fixtures.bobClient.address, timestamp: timestamp)
        let signature = try await fixtures.alice.sign(message: signatureText)
        
        let hex = signature.rawData.toHex
        var consentProofPayload = ConsentProofPayload()
        consentProofPayload.signature = hex
        consentProofPayload.timestamp = timestamp
        consentProofPayload.payloadVersion = .consentProofPayloadVersion1
        let boConversation =
		try await fixtures.bobClient.conversations.newConversation(with: fixtures.aliceClient.address, context: nil, consentProofPayload: consentProofPayload)
        let alixConversations = try await
		fixtures.aliceClient.conversations.list()
        let alixConversation = alixConversations.first(where: { $0.topic == boConversation.topic })
        XCTAssertNotNil(alixConversation)
        let consentStatus = try await fixtures.aliceClient.contacts.isAllowed(fixtures.bobClient.address)
        XCTAssertTrue(consentStatus)
    }

    func testNetworkConsentOverConsentProof() async throws {
        let fixtures = await fixtures()

        let timestamp = UInt64(Date().timeIntervalSince1970 * 1000)
        let signatureText = Signature.consentProofText(peerAddress: fixtures.bobClient.address, timestamp: timestamp)
        let signature = try await fixtures.alice.sign(message: signatureText)
        let hex = signature.rawData.toHex
        var consentProofPayload = ConsentProofPayload()
        consentProofPayload.signature = hex
        consentProofPayload.timestamp = timestamp
        consentProofPayload.payloadVersion = .consentProofPayloadVersion1
        let boConversation =
        try await fixtures.bobClient.conversations.newConversation(with: fixtures.aliceClient.address, context: nil, consentProofPayload: consentProofPayload)
        try await fixtures.aliceClient.contacts.deny(addresses: [fixtures.bobClient.address])
        let alixConversations = try await
			fixtures.aliceClient.conversations.list()
        let alixConversation = alixConversations.first(where: { $0.topic == boConversation.topic })
        XCTAssertNotNil(alixConversation)
        let isDenied = try await fixtures.aliceClient.contacts.isDenied(fixtures.bobClient.address)
        XCTAssertTrue(isDenied)
    }
    
    func testConsentProofInvalidSignature() async throws {
		throw XCTSkip("this test is flakey in CI, TODO: figure it out")
        let fixtures = await fixtures()

        let timestamp = UInt64(Date().timeIntervalSince1970 * 1000)
        let signatureText = Signature.consentProofText(peerAddress: fixtures.bobClient.address, timestamp: timestamp + 1)
        let signature = try await fixtures.alice.sign(message:signatureText)
        let hex = signature.rawData.toHex
        var consentProofPayload = ConsentProofPayload()
        consentProofPayload.signature = hex
        consentProofPayload.timestamp = timestamp
        consentProofPayload.payloadVersion = .consentProofPayloadVersion1
        let boConversation =
        try await fixtures.bobClient.conversations.newConversation(with: fixtures.aliceClient.address, context: nil, consentProofPayload: consentProofPayload)
        let alixConversations = try await
			fixtures.aliceClient.conversations.list()
        let alixConversation = alixConversations.first(where: { $0.topic == boConversation.topic })
        XCTAssertNotNil(alixConversation)
        let isAllowed = try await fixtures.aliceClient.contacts.isAllowed(fixtures.bobClient.address)
        XCTAssertFalse(isAllowed)
    }
}
