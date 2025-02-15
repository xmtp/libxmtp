import CryptoKit
import CryptoSwift
import Foundation
import LibXMTP

public let ContentTypeMultiRemoteAttachment = ContentTypeID(authorityID: "xmtp.org", typeID: "multiRemoteStaticAttachment", versionMajor: 1, versionMinor: 0)

public enum MultiRemoteAttachmentError: Error, CustomStringConvertible {
    case invalidURL, v1NotSupported, invalidParameters(String), invalidDigest(String), invalidScheme(String), payloadNotFound
    
    public var description: String {
        switch self {
        case .invalidURL:
            return "MultiRemoteAttachmentError.invalidURL"
        case .v1NotSupported:
            return "MultiRemoteAttachmentError.v1NotSupported"
        case .invalidParameters(let string):
            return "MultiRemoteAttachmentError.invalidParameters: \(string)"
        case .invalidDigest(let string):
            return "MultiRemoteAttachmentError.invalidDigest: \(string)"
        case .invalidScheme(let string):
            return "MultiRemoteAttachmentError.invalidScheme: \(string)"
        case .payloadNotFound:
            return "MultiRemoteAttachmentError.payloadNotFound"
        }
    }
}

public struct MultiRemoteAttachment {
    public enum Scheme: String {
        case https = "https"
    }
    
    public let remoteAttachments: [RemoteAttachmentInfo]

	public init(remoteAttachments: [RemoteAttachmentInfo]) {
        self.remoteAttachments = remoteAttachments
    }
    
    public struct RemoteAttachmentInfo {
        public let url: String
        public let filename: String
        public let contentLength: UInt32
        public let contentDigest: String
        public let nonce: Data
        public let scheme: String
        public let salt: Data
        public let secret: Data
        
        public init(
            url: String,
            filename: String,
            contentLength: UInt32,
            contentDigest: String,
            nonce: Data,
            scheme: String,
            salt: Data,
            secret: Data
        ) {
            self.url = url
            self.filename = filename
            self.contentLength = contentLength
            self.contentDigest = contentDigest
            self.nonce = nonce
            self.scheme = scheme
            self.salt = salt
            self.secret = secret
        }
        
        public static func from(url: URL, encryptedEncodedContent: EncryptedEncodedContent) throws -> RemoteAttachmentInfo {
            guard url.scheme == "https" else {
                throw MultiRemoteAttachmentError.invalidScheme("scheme must be https://")
            }
            
            return RemoteAttachmentInfo(
                url: url.absoluteString,
                filename: encryptedEncodedContent.filename ?? "",
                contentLength: encryptedEncodedContent.contentLength ?? 0,
                contentDigest: encryptedEncodedContent.digest,
                nonce: encryptedEncodedContent.nonce,
                scheme: url.scheme ?? "https",
                salt: encryptedEncodedContent.salt,
                secret: encryptedEncodedContent.secret
            )
        }
    }
}

public struct MultiRemoteAttachmentCodec: ContentCodec {
    
    public typealias T = MultiRemoteAttachment
    
    public init() {}
    
    public var contentType = ContentTypeMultiRemoteAttachment
    
    public func encode(content: MultiRemoteAttachment) throws -> EncodedContent {
        let ffiMultiRemoteAttachment = FfiMultiRemoteAttachment(attachments: content.remoteAttachments.map { FfiRemoteAttachmentInfo(
            secret: $0.secret,
            contentDigest: $0.contentDigest,
            nonce: $0.nonce,
            scheme: $0.scheme,
            url: $0.url,
            salt: $0.salt,
            contentLength: $0.contentLength,
            filename: $0.filename
        )})
        return try EncodedContent(serializedBytes: encodeMultiRemoteAttachment(ffiMultiRemoteAttachment: ffiMultiRemoteAttachment))
    }
    
    public func decode(content: EncodedContent) throws -> MultiRemoteAttachment {
       
        let ffiMultiRemoteAttachment = try decodeMultiRemoteAttachment(bytes: content.serializedData())
        let remoteAttachments = ffiMultiRemoteAttachment.attachments.map { attachment in
            return MultiRemoteAttachment.RemoteAttachmentInfo(
                url: attachment.url,
                filename: attachment.filename ?? "",
                contentLength: attachment.contentLength ?? 0,
                contentDigest: attachment.contentDigest,
                nonce: attachment.nonce,
                scheme: attachment.scheme,
                salt: attachment.salt,
                secret: attachment.secret
            )
        }
        return MultiRemoteAttachment(remoteAttachments: remoteAttachments)
    }
    
    public func fallback(content: MultiRemoteAttachment) throws -> String? {
        return "MultiRemoteAttachment not supported"
    }
    
    public func shouldPush(content: MultiRemoteAttachment) throws -> Bool {
        return true
    }
    
    /// Encrypt arbitrary bytes (already encoded with some codec, e.g. AttachmentCodec)
    /// so that they can be uploaded to remote storage.
    static func encryptBytesForLocalAttachment(_ bytesToEncrypt: Data, filename: String) throws -> EncryptedEncodedContent {
        return try RemoteAttachment.encodeEncryptedBytes(encodedContent: bytesToEncrypt, filename: filename)
    }
    
    /// Builds a `RemoteAttachmentInfo` from an `EncryptedEncodedContent` plus a remote HTTPS URL.
    static func buildRemoteAttachmentInfo(
        encryptedAttachment: EncryptedEncodedContent,
        remoteUrl: URL
    ) throws -> MultiRemoteAttachment.RemoteAttachmentInfo {
        return try MultiRemoteAttachment.RemoteAttachmentInfo.from(url: remoteUrl, encryptedEncodedContent: encryptedAttachment)
    }
    
    /// Build a new `EncryptedEncodedContent` by combining the original `RemoteAttachment`
    /// fields (digest, salt, nonce, etc.) with the actual encrypted payload you downloaded (or stored).
    static func buildEncryptAttachmentResult(
        remoteAttachment: RemoteAttachment,
        encryptedPayload: Data
    ) -> EncryptedEncodedContent {
        return EncryptedEncodedContent(
            secret: remoteAttachment.secret,
            digest: remoteAttachment.contentDigest,
            salt: remoteAttachment.salt,
            nonce: remoteAttachment.nonce,
            payload: encryptedPayload,
            filename: remoteAttachment.filename,
            contentLength: remoteAttachment.contentLength.map(UInt32.init)
        )
    }
    
    /// Decrypt an `EncryptedEncodedContent` back to the underlying `EncodedContent`.
    static func decryptAttachment(_ encryptedAttachment: EncryptedEncodedContent) throws -> EncodedContent {
        // Delegate to a "decryptEncoded" function from `RemoteAttachment`, if you have it.
        // Alternatively, replicate the same logic as `RemoteAttachment.decodeEncrypted(...)`.
        return try RemoteAttachment.decryptEncoded(encrypted: encryptedAttachment)
    }
}
