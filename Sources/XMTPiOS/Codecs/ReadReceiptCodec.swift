//
//  ReadReceiptCodec.swift
//  
//
//  Created by Naomi Plasterer on 8/2/23.
//

import Foundation

public let ContentTypeReadReceipt = ContentTypeID(authorityID: "xmtp.org", typeID: "readReceipt", versionMajor: 1, versionMinor: 0)

public struct ReadReceipt {
    public init() {}
}

public struct ReadReceiptCodec: ContentCodec {
    public typealias T = ReadReceipt

    public init() {}

    public var contentType = ContentTypeReadReceipt

    public func encode(content: ReadReceipt) throws -> EncodedContent {
        var encodedContent = EncodedContent()

        encodedContent.type = ContentTypeReadReceipt
        encodedContent.content = Data()

        return encodedContent
    }

    public func decode(content: EncodedContent) throws -> ReadReceipt {
        return ReadReceipt()
    }

    public func fallback(content: ReadReceipt) throws -> String? {
        return nil
    }

	public func shouldPush(content: ReadReceipt) throws -> Bool {
		return false
	}
}
