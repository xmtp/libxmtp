//
//  ReadReceiptTests.swift
//  
//
//  Created by Naomi Plasterer on 8/2/23.
//

import Foundation

import XCTest
@testable import XMTP

@available(iOS 15, *)
class ReadReceiptTests: XCTestCase {
    func testCanUseReadReceiptCodec() async throws {
        Client.register(codec: ReadReceiptCodec())
        
        let fixtures = await fixtures()
        let conversation = try await fixtures.aliceClient.conversations.newConversation(with: fixtures.bobClient.address)

        try await conversation.send(text: "hey alice 2 bob")

        let read = ReadReceipt()

        try await conversation.send(
            content: read,
            options: .init(contentType: ContentTypeReadReceipt)
        )

        let updatedMessages = try await conversation.messages()
        
        let message = try await conversation.messages()[0]
        let contentType: String = message.encodedContent.type.typeID
        XCTAssertEqual("readReceipt", contentType)
    }
}
