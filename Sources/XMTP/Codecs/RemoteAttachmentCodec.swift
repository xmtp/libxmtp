//
//  RemoteAttachmentCodec.swift
//
//
//  Created by Pat Nakajima on 2/19/23.
//

import CryptoKit
import Foundation
import web3

public let ContentTypeRemoteAttachment = ContentTypeID(authorityID: "xmtp.org", typeID: "remoteStaticAttachment", versionMajor: 1, versionMinor: 0)

public enum RemoteAttachmentError: Error {
	case invalidURL, v1NotSupported, invalidParameters(String), invalidDigest(String), invalidScheme(String), payloadNotFound
}

protocol RemoteContentFetcher {
	func fetch(_ url: String) async throws -> Data
}

struct HTTPFetcher: RemoteContentFetcher {
	func fetch(_ url: String) async throws -> Data {
		guard let url = URL(string: url) else {
			throw RemoteAttachmentError.invalidURL
		}

		return try await URLSession.shared.data(from: url).0
	}
}

public struct RemoteAttachment {
	public enum Scheme: String {
		case https = "https"
	}

	public var url: String
	public var contentDigest: String
	public var secret: Data
	public var salt: Data
	public var nonce: Data
	public var scheme: Scheme
	var fetcher: RemoteContentFetcher

	// Non standard
	public var contentLength: Int?
	public var filename: String?

	public init(url: String, contentDigest: String, secret: Data, salt: Data, nonce: Data, scheme: Scheme) throws {
		self.url = url
		self.contentDigest = contentDigest
		self.secret = secret
		self.salt = salt
		self.nonce = nonce

		self.scheme = scheme
		fetcher = HTTPFetcher()

		try ensureSchemeMatches()
	}

	public init(url: String, encryptedEncodedContent: EncryptedEncodedContent) throws {
		self.url = url
		contentDigest = encryptedEncodedContent.digest
		secret = encryptedEncodedContent.secret
		salt = encryptedEncodedContent.salt
		nonce = encryptedEncodedContent.nonce

		scheme = .https
		fetcher = HTTPFetcher()

		try ensureSchemeMatches()
	}

	func ensureSchemeMatches() throws {
		if !url.hasPrefix(scheme.rawValue) {
			throw RemoteAttachmentError.invalidScheme("scheme must be https")
		}
	}

	public static func encodeEncrypted<Codec: ContentCodec, T>(content: T, codec: Codec, with client: Client) throws -> EncryptedEncodedContent where Codec.T == T {
		let secret = try Crypto.secureRandomBytes(count: 32)
		let encodedContent = try codec.encode(content: content, client: client).serializedData()
		let ciphertext = try Crypto.encrypt(secret, encodedContent)
		let contentDigest = sha256(data: ciphertext.aes256GcmHkdfSha256.payload)

		return EncryptedEncodedContent(
			secret: secret,
			digest: contentDigest,
			salt: ciphertext.aes256GcmHkdfSha256.hkdfSalt,
			nonce: ciphertext.aes256GcmHkdfSha256.gcmNonce,
			payload: ciphertext.aes256GcmHkdfSha256.payload
		)
	}

	public static func decryptEncoded(encrypted: EncryptedEncodedContent) throws -> EncodedContent {
		if RemoteAttachment.sha256(data: encrypted.payload) != encrypted.digest {
			throw RemoteAttachmentError.invalidDigest("content digest does not match. expected: \(encrypted.digest), got: \(SHA256.hash(data: encrypted.payload).description)")
		}

		let decrypted = try Crypto.decrypt(encrypted.secret, CipherText.with {
			let aes256GcmHkdfSha256 = CipherText.Aes256gcmHkdfsha256.with { aes in
				aes.hkdfSalt = encrypted.salt
				aes.gcmNonce = encrypted.nonce
				aes.payload = encrypted.payload
			}

			$0.aes256GcmHkdfSha256 = aes256GcmHkdfSha256
		})
		return try EncodedContent(serializedData: decrypted)
	}

	public func content() async throws -> EncodedContent {
		let payload = try await fetcher.fetch(url)

		if payload.isEmpty {
			throw RemoteAttachmentError.payloadNotFound
		}

		return try RemoteAttachment.decryptEncoded(encrypted: EncryptedEncodedContent(
				secret: secret,
				digest: contentDigest,
				salt: salt,
				nonce: nonce,
				payload: payload
		))
	}

	private static func sha256(data: Data) -> String {
		Data(SHA256.hash(data: data)).toHex
	}
}

public struct RemoteAttachmentCodec: ContentCodec {
    
	public typealias T = RemoteAttachment

	public init() {}

	public var contentType = ContentTypeRemoteAttachment

	public func encode(content: RemoteAttachment, client _: Client) throws -> EncodedContent {
		var encodedContent = EncodedContent()

		encodedContent.type = ContentTypeRemoteAttachment
		encodedContent.content = Data(content.url.utf8)
		encodedContent.parameters = [
			"contentDigest": content.contentDigest,
			"secret": content.secret.toHex,
			"salt": content.salt.toHex,
			"nonce": content.nonce.toHex,
			"scheme": "https",
			"contentLength": String(content.contentLength ?? -1),
			"filename": content.filename ?? "",
		]

		return encodedContent
	}

	public func decode(content: EncodedContent, client _: Client) throws -> RemoteAttachment {
		guard let url = String(data: content.content, encoding: .utf8) else {
			throw RemoteAttachmentError.invalidURL
		}

		guard let contentDigest = content.parameters["contentDigest"] else {
			throw RemoteAttachmentError.invalidDigest("missing contentDigest parameter")
		}

		let secret = try getHexParameter("secret", from: content.parameters)
		let salt = try getHexParameter("salt", from: content.parameters)
		let nonce = try getHexParameter("nonce", from: content.parameters)

		guard let schemeString = content.parameters["scheme"] else {
			throw RemoteAttachmentError.invalidScheme("no scheme parameter")
		}

        if (!schemeString.hasPrefix(RemoteAttachment.Scheme.https.rawValue)) {
			throw RemoteAttachmentError.invalidScheme("invalid scheme value. must start with https")
		}

		var attachment = try RemoteAttachment(url: url, contentDigest: contentDigest, secret: secret, salt: salt, nonce: nonce, scheme: RemoteAttachment.Scheme.https)

		if let contentLength = content.parameters["contentLength"] {
			attachment.contentLength = Int(contentLength)
		}

		if let filename = content.parameters["filename"] {
			attachment.filename = filename
		}

		return attachment
	}
    
    public func fallback(content: RemoteAttachment) throws -> String? {
        return "Can’t display “\(String(describing: content.filename))”. This app doesn’t support attachments."
    }

	private func getHexParameter(_ name: String, from parameters: [String: String]) throws -> Data {
		guard let parameterHex = parameters[name] else {
			throw RemoteAttachmentError.invalidParameters("missing \(name) parameter")
		}

		guard let parameterData = parameterHex.web3.hexData else {
			throw RemoteAttachmentError.invalidParameters("invalid \(name) value")
		}

		return Data(parameterData)
	}
}
