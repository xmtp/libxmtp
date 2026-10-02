import Foundation

public extension RemoteAttachment {
    /// Build the attachment record from Rust-encrypted content.
    init(url: String, encryptedEncodedContent: EncryptedEncodedContent, filename: String? = nil) throws {
        self = try remoteAttachmentFromEncrypted(
            url: url, encryptedEncodedContent: encryptedEncodedContent, filename: filename
        )
    }
}
