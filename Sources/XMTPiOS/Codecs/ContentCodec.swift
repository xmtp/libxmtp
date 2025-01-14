//
//  ContentCodec.swift
//
//
//  Created by Pat Nakajima on 11/28/22.
//

import Foundation

enum CodecError: String, Error {
	case invalidContent, codecNotFound
}

public typealias EncodedContent = Xmtp_MessageContents_EncodedContent

extension EncodedContent {
	public func decoded<T>() throws -> T {
		let codec = Client.codecRegistry.find(for: type)

		var encodedContent = self

		if hasCompression {
			encodedContent = try decompressContent()
		}

		if let content = try codec.decode(content: encodedContent) as? T {
			return content
		}

		throw CodecError.invalidContent
	}

	func compress(_ compression: EncodedContentCompression) throws -> EncodedContent {
		var copy = self

		switch compression {
		case .deflate:
			copy.compression = .deflate
		case .gzip:
			copy.compression = .gzip
		}

		if let compressedContent = compression.compress(content: content) {
			copy.content = compressedContent
		} else {
			throw CodecError.invalidContent
		}

		return copy
	}

	func decompressContent() throws -> EncodedContent {
		if !hasCompression {
			return self
		}

		var copy = self

		switch compression {
		case .gzip:
			if let decompressedContent = EncodedContentCompression.gzip.decompress(content: content) {
				copy.content = decompressedContent
			} else {
				throw CodecError.invalidContent
			}
		case .deflate:
			if let decompressedContent = EncodedContentCompression.deflate.decompress(content: content) {
				copy.content = decompressedContent
			} else {
				throw CodecError.invalidContent
			}
		default:
			return copy
		}

		return copy
	}

}

public protocol ContentCodec: Hashable, Equatable {
	associatedtype T

	var contentType: ContentTypeID { get }
	func encode(content: T) throws -> EncodedContent
	func decode(content: EncodedContent) throws -> T
	func fallback(content: T) throws -> String?
	func shouldPush(content: T) throws -> Bool
}

public extension ContentCodec {
	static func == (lhs: Self, rhs: Self) -> Bool {
		return lhs.contentType.authorityID == rhs.contentType.authorityID && lhs.contentType.typeID == rhs.contentType.typeID
	}

	var id: String {
		contentType.id
	}

	func hash(into hasher: inout Hasher) {
		hasher.combine(id)
	}

	var description: String {
		contentType.description
	}
}
