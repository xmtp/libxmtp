//
//  ReactionV2Codec.swift
//
//
//  Created by Naomi Plasterer on 7/26/23.
//

import Foundation

public let ContentTypeReactionV2 = ContentTypeID(
	authorityID: "xmtp.org",
	typeID: "reaction",
	versionMajor: 2,
	versionMinor: 0
)

public struct ReactionV2Codec: ContentCodec {
	public typealias T = Reaction
	public var contentType = ContentTypeReactionV2

	public init() {}

	public func encode(content: Reaction) throws -> EncodedContent {
		// Convert Reaction to FfiReactionPayload for encoding
		let ffiReaction = FfiReactionPayload(
			reference: content.reference,
			referenceInboxId: content.referenceInboxId ?? "",
			action: content.action.toFfiReactionAction(),
			content: content.content,
			schema: content.schema.toFfiReactionSchema()
		)
		return try EncodedContent(serializedBytes: encodeReaction(reaction: ffiReaction))
	}

	public func decode(content: EncodedContent) throws -> Reaction {
		let ffiReaction = try decodeReaction(bytes: content.serializedBytes())
		// Convert FfiReactionPayload to Reaction
		return Reaction(
			reference: ffiReaction.reference,
			action: ReactionAction.fromFfiReactionAction(ffiReaction.action),
			content: ffiReaction.content,
			schema: ReactionSchema.fromFfiReactionSchema(ffiReaction.schema),
			referenceInboxId: ffiReaction.referenceInboxId
		)
	}

	public func fallback(content: Reaction) throws -> String? {
		switch content.action {
		case .added:
			return "Reacted \"\(content.content)\" to an earlier message"
		case .removed:
			return "Removed \"\(content.content)\" from an earlier message"
		case .unknown:
			return nil
		}
	}

	public func shouldPush(content: Reaction) throws -> Bool {
		switch content.action {
		case .added:
			return true
		default:
			return false
		}
	}
}

// MARK: - Conversion Extensions

extension ReactionAction {
	func toFfiReactionAction() -> FfiReactionAction {
		switch self {
		case .added:
			return .added
		case .removed:
			return .removed
		case .unknown:
			return .unknown
		}
	}

	static func fromFfiReactionAction(_ action: FfiReactionAction) -> ReactionAction {
		switch action {
		case .added:
			return .added
		case .removed:
			return .removed
		case .unknown:
			return .unknown
		}
	}
}

extension ReactionSchema {
	func toFfiReactionSchema() -> FfiReactionSchema {
		switch self {
		case .unicode:
			return .unicode
		case .shortcode:
			return .shortcode
		case .custom:
			return .custom
		case .unknown:
			return .unknown
		}
	}

	static func fromFfiReactionSchema(_ schema: FfiReactionSchema) -> ReactionSchema {
		switch schema {
		case .unicode:
			return .unicode
		case .shortcode:
			return .shortcode
		case .custom:
			return .custom
		case .unknown:
			return .unknown
		}
	}
}
