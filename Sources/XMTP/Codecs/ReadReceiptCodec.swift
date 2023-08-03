//
//  ReadReceiptCodec.swift
//  
//
//  Created by Naomi Plasterer on 8/2/23.
//

import Foundation

public let ContentTypeReadReceipt = ContentTypeID(authorityID: "xmtp.org", typeID: "readReceipt", versionMajor: 1, versionMinor: 0)

public struct ReadReceipt {
    public var timestamp: String
    
    public init(timestamp: String) {
        self.timestamp = timestamp
    }
}

public struct ReadReceiptCodec: ContentCodec {
    public typealias T = ReadReceipt

    public init() {}

    public var contentType = ContentTypeReadReceipt

    public func encode(content: ReadReceipt) throws -> EncodedContent {
        var encodedContent = EncodedContent()

        encodedContent.type = ContentTypeReadReceipt
        encodedContent.parameters = ["timestamp": content.timestamp]
        encodedContent.content = Data()

        return encodedContent
    }

    public func decode(content: EncodedContent) throws -> ReadReceipt {
        guard let timestamp = content.parameters["timestamp"] else {
            throw CodecError.invalidContent
        }

        return ReadReceipt(timestamp: timestamp)
    }
}
