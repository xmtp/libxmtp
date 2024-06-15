//
//  FramesTests.swift
//
//
//  Created by Alex Risch on 4/1/24.
//

import Foundation
import secp256k1
import XCTest
@testable import XMTPiOS

final class FramesTests: XCTestCase {
    func testInstantiateFramesClient() async throws {
        let frameUrl = "https://fc-polls-five.vercel.app/polls/01032f47-e976-42ee-9e3d-3aac1324f4b8"
        
        let key = try Crypto.secureRandomBytes(count: 32)
        let bo = try PrivateKey.generate()
        let client = try await Client.create(
            account: bo,
            options: .init(
                api: .init(env: .local, isSecure: false),
				enableV3: true,
                encryptionKey: key
            )
        )

        let framesClient = FramesClient(xmtpClient: client)
        let metadata = try await framesClient.proxy.readMetadata(url: frameUrl)
        let conversationTopic = "foo"
        let participantAccountAddresses =  ["amal", "bola"]
        let dmInputs = DmActionInputs(
        conversationTopic: conversationTopic, participantAccountAddresses: participantAccountAddresses)
        let conversationInputs = ConversationActionInputs.dm(dmInputs)
        let frameInputs = FrameActionInputs(frameUrl: frameUrl, buttonIndex: 1, inputText: nil, state: nil, conversationInputs: conversationInputs)
        let signedPayload = try await framesClient.signFrameAction(inputs: frameInputs)
            
        guard let postUrl = metadata.extractedTags["fc:frame:post_url"] else {
            throw NSError(domain: "", code: 0, userInfo: [NSLocalizedDescriptionKey: "postUrl should exist"])
        }
        let response = try await framesClient.proxy.post(url: postUrl, payload: signedPayload)
            
        guard response.extractedTags["fc:frame"] == "vNext" else {
            throw NSError(domain: "", code: 0, userInfo: [NSLocalizedDescriptionKey: "response should have expected extractedTags"])
        }
            
        guard let imageUrl = response.extractedTags["fc:frame:image"] else {
            throw NSError(domain: "", code: 0, userInfo: [NSLocalizedDescriptionKey: "imageUrl should exist"])
        }
            
        let mediaUrl = try await framesClient.proxy.mediaUrl(url: imageUrl)
            
        let (_, mediaResponse) = try await URLSession.shared.data(from: URL(string: mediaUrl)!)
            
        guard (mediaResponse as? HTTPURLResponse)?.statusCode == 200 else {
            throw NSError(domain: "", code: 0, userInfo: [NSLocalizedDescriptionKey: "downloadedMedia should be ok"])
        }
            
        guard (mediaResponse as? HTTPURLResponse)?.mimeType == "image/png" else {
            throw NSError(domain: "", code: 0, userInfo: [NSLocalizedDescriptionKey: "downloadedMedia should be image/png"])
        }
    }
}
