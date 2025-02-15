import Foundation
import XCTest

@testable import XMTPiOS

@available(macOS 13.0, *)
@available(iOS 16, *)
class MultiRemoteAttachmentTests: XCTestCase {
    
    func testCanEncryptAndDecrypt() async throws {
        Client.register(codec: AttachmentCodec())
        Client.register(codec: RemoteAttachmentCodec())
        Client.register(codec: MultiRemoteAttachmentCodec())
        
        // Create an Attachment
        let originalAttachment = Attachment(
            filename: "test.txt",
            mimeType: "text/plain",
            data: Data("hello world".utf8)
        )
        
        // Convert attachment into an encryptedEncodedContent
        let encryptedEncodedContent = try RemoteAttachment.encodeEncrypted(
            content: originalAttachment,
            codec: AttachmentCodec()
        )
        
        // Now decrypt via decryptAttachment
        let decryptedEncodedContent: EncodedContent = try MultiRemoteAttachmentCodec.decryptAttachment(encryptedEncodedContent)
        
        // Finally decode
        let decodedAttachment: Attachment = try decryptedEncodedContent.decoded()
        XCTAssertEqual(decodedAttachment.data, originalAttachment.data)
    }
    
    
    func testCanUseMultiRemoteAttachmentCodec() async throws {
        let fixtures = try await fixtures()
        
        let alixClient = fixtures.alixClient!
        let boClient = fixtures.boClient!
        
        // Register all necessary codecs
        Client.register(codec: AttachmentCodec())
        Client.register(codec: RemoteAttachmentCodec())
        Client.register(codec: MultiRemoteAttachmentCodec())
        
        // Create a conversation
        let alixConversation = try await alixClient.conversations
            .newConversation(with: boClient.address)
        
        // Create some dummy attachments to send
        let attachment1 = Attachment(
            filename: "test1.txt",
            mimeType: "text/plain",
            data: Data("hello world".utf8)
        )
        
        let attachment2 = Attachment(
            filename: "test2.txt",
            mimeType: "text/plain",
            data: Data("hello world".utf8)
        )
        
        // We'll store the encrypted payloads in a local dictionary keyed by a fake https URL
        var encryptedPayloads: [String: Data] = [:]
        
        func fakeUpload(_ encryptedPayload: Data) -> String {
            // Generate a random https:// URL for simulation
            let randomNumber = Int.random(in: 0..<1_000_000)
            let urlString = "https://\(randomNumber)"
            encryptedPayloads[urlString] = encryptedPayload
            return urlString
        }
        
        // Convert attachments -> EncryptedEncodedContent -> RemoteAttachmentInfo
        var remoteAttachmentInfos: [MultiRemoteAttachment.RemoteAttachmentInfo] = []
        
        for att in [attachment1, attachment2] {
            // 1) Encode the attachment to raw bytes
            let encodedBytes = try AttachmentCodec().encode(content: att).serializedData()
            // 2) Encrypt the bytes locally
            let encrypted = try MultiRemoteAttachmentCodec.encryptBytesForLocalAttachment(encodedBytes, filename: att.filename)
            // 3) “Upload” it, and get a random https:// URL back
            let urlString = fakeUpload(encrypted.payload)
            // 4) Build a RemoteAttachmentInfo for that URL
            let info = try MultiRemoteAttachmentCodec.buildRemoteAttachmentInfo(
                encryptedAttachment: encrypted,
                remoteUrl: URL(string: urlString)!
            )
            remoteAttachmentInfos.append(info)
        }
        
        XCTAssertEqual(remoteAttachmentInfos.count, 2)
        
        // Wrap them up in a single MultiRemoteAttachment
        let multiRemoteAttachment = MultiRemoteAttachment(remoteAttachments: remoteAttachmentInfos)
        
        // Add debugging checks before sending
        let encodedContent = try MultiRemoteAttachmentCodec().encode(content: multiRemoteAttachment)

        
        // Example usage:
        let isRegistered1 = Client.codecRegistry.isRegistered(codec: AttachmentCodec())
        // or
        let isRegistered2 = Client.codecRegistry.isRegistered(codecId: "xmtp.org:multiRemoteStaticAttachment:1.0")
        XCTAssertTrue(isRegistered1)
        XCTAssertTrue(isRegistered2)
        
        // Try sending with the debugged encoded content
        _ = try await alixConversation.send(encodedContent: encodedContent)

        // Fetch messages
        try await alixConversation.sync()
        let messages = try await alixConversation.messages()
        XCTAssertEqual(messages.count, 1)
        
        let received = messages[0]
        XCTAssertEqual(try received.encodedContent.type.id, ContentTypeMultiRemoteAttachment.id)
        
                // Decode the raw content back into a MultiRemoteAttachment
                let loadedMultiRemoteAttachment: MultiRemoteAttachment = try received.content()
        
                XCTAssertEqual(loadedMultiRemoteAttachment.remoteAttachments.count, 2)
        
                // Now simulate how we handle each remote attachment
                var decodedAttachments: [Attachment] = []
                for info in loadedMultiRemoteAttachment.remoteAttachments {
                    // Build a single RemoteAttachment object to mirror the Android code
                    let remoteAttachment = try RemoteAttachment(
                        url: info.url,
                        contentDigest: info.contentDigest,
                        secret: info.secret,
                        salt: info.salt,
                        nonce: info.nonce,
                        scheme: RemoteAttachment.Scheme(rawValue: info.scheme) ?? .https, // Convert string to enum
                        contentLength: Int(info.contentLength),
                        filename: info.filename
                    )
        
                    // “Download” the encrypted payload
                    guard let downloadedPayload = encryptedPayloads[remoteAttachment.url] else {
                        XCTFail("No stored payload for \(remoteAttachment.url)")
                        return
                    }
        
                    // Recombine that payload with the attachment’s metadata
                    let encryptedAttachment = MultiRemoteAttachmentCodec.buildEncryptAttachmentResult(
                        remoteAttachment: remoteAttachment,
                        encryptedPayload: downloadedPayload
                    )
        
                    // Decrypt
                    let decodedContent = try MultiRemoteAttachmentCodec.decryptAttachment(encryptedAttachment)
        
                    // Confirm it’s text/plain or something we expect
                    XCTAssertEqual(decodedContent.type.id, ContentTypeAttachment.id)
        
                    // Decode the final `EncodedContent` as an `Attachment`
                    let finalAttachment: Attachment = try decodedContent.decoded()
                    decodedAttachments.append(finalAttachment)
                }
        
                XCTAssertEqual(decodedAttachments.count, 2)
                XCTAssertEqual(decodedAttachments[0].filename, "test1.txt")
                XCTAssertEqual(decodedAttachments[1].filename, "test2.txt")
    }
}

