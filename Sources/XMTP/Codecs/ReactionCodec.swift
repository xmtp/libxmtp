//
//  ReactionCodec.swift
//  
//
//  Created by Naomi Plasterer on 7/26/23.
//

import Foundation


public let ContentTypeReaction = ContentTypeID(authorityID: "xmtp.org", typeID: "reaction", versionMajor: 1, versionMinor: 0)

public struct Reaction: Codable {
    public var reference: String
    public var action: ReactionAction
    public var content: String
    public var schema: ReactionSchema
    
    public init(reference: String, action: ReactionAction, content: String, schema: ReactionSchema) {
        self.reference = reference
        self.action = action
        self.content = content
        self.schema = schema
    }
}

public enum ReactionAction: String, Codable {
    case added, removed
}

public enum ReactionSchema: String, Codable {
    case unicode, shortcode, custom
}

public struct ReactionCodec: ContentCodec {
    public typealias T = Reaction
    public var contentType = ContentTypeReaction

    public init() {}

    public func encode(content: Reaction) throws -> EncodedContent {
        var encodedContent = EncodedContent()

        encodedContent.type = ContentTypeReaction
        encodedContent.content = try JSONEncoder().encode(content)

        return encodedContent
    }

    public func decode(content: EncodedContent) throws -> Reaction {
        // First try to decode it in the canonical form.
        // swiftlint:disable no_optional_try
        if let reaction = try? JSONDecoder().decode(Reaction.self, from: content.content) {
            return reaction
        }
        // swiftlint:disable no_optional_try
        // If that fails, try to decode it in the legacy form.
        // swiftlint:disable force_unwrapping
        return Reaction(
            reference: content.parameters["reference"] ?? "",
            action: ReactionAction(rawValue: content.parameters["action"] ?? "")!,
            content: String(data: content.content, encoding: .utf8) ?? "",
            schema: ReactionSchema(rawValue: content.parameters["schema"] ?? "")!
        )
        //swiftlint:disable force_unwrapping
    }
}
