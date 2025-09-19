//
//  ReactionCodec.swift
//  
//
//  Created by Naomi Plasterer on 7/26/23.
//

import Foundation
import LibXMTP

public let ContentTypeReactionV2 = ContentTypeID(authorityID: "xmtp.org", typeID: "reaction", versionMajor: 2, versionMinor: 0)

public struct ReactionV2Codec: ContentCodec {
    public typealias T = FfiReactionPayload
    public var contentType = ContentTypeReactionV2

    public init() {}

    public func encode(content: FfiReactionPayload) throws -> EncodedContent {
        return try EncodedContent(serializedBytes: LibXMTP.encodeReaction(reaction: content))
    }

    public func decode(content: EncodedContent) throws -> FfiReactionPayload {
        try LibXMTP.decodeReaction(bytes: content.serializedBytes())
    }

    public func fallback(content: FfiReactionPayload) throws -> String? {
        switch content.action {
        case .added:
            return "Reacted \"\(content.content)\" to an earlier message"
        case .removed:
            return "Removed \"\(content.content)\" from an earlier message"
        case .unknown:
            return nil
        }
    }

	public func shouldPush(content: FfiReactionPayload) throws -> Bool {
        switch content.action {
        case .added:
            return true
        default:
            return false
        }
	}
}
