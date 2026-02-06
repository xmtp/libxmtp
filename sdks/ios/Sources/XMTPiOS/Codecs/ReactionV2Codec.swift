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
	versionMinor: 0,
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
			schema: content.schema.toFfiReactionSchema(),
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
			referenceInboxId: ffiReaction.referenceInboxId,
		)
	}

	public func fallback(content: Reaction) throws -> String? {
		switch content.action {
		case .added:
			"Reacted \"\(content.content)\" to an earlier message"
		case .removed:
			"Removed \"\(content.content)\" from an earlier message"
		case .unknown:
			nil
		}
	}

	public func shouldPush(content: Reaction) throws -> Bool {
		switch content.action {
		case .added:
			true
		default:
			false
		}
	}
}

// MARK: - Conversion Extensions

extension ReactionAction {
	func toFfiReactionAction() -> FfiReactionAction {
		switch self {
		case .added:
			.added
		case .removed:
			.removed
		case .unknown:
			.unknown
		}
	}

	static func fromFfiReactionAction(_ action: FfiReactionAction) -> ReactionAction {
		switch action {
		case .added:
			.added
		case .removed:
			.removed
		case .unknown:
			.unknown
		}
	}
}

extension ReactionSchema {
	func toFfiReactionSchema() -> FfiReactionSchema {
		switch self {
		case .unicode:
			.unicode
		case .shortcode:
			.shortcode
		case .custom:
			.custom
		case .unknown:
			.unknown
		}
	}

	static func fromFfiReactionSchema(_ schema: FfiReactionSchema) -> ReactionSchema {
		switch schema {
		case .unicode:
			.unicode
		case .shortcode:
			.shortcode
		case .custom:
			.custom
		case .unknown:
			.unknown
		}
	}
}
