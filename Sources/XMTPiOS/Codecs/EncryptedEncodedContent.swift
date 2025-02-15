//
//  EncryptedEncodedContent.swift
//
//
//  Created by Pat on 2/21/23.
//

import Foundation

public struct EncryptedEncodedContent {
	public var secret: Data
	public var digest: String
	public var salt: Data
	public var nonce: Data
	public var payload: Data
    public var filename: String?
    public var contentLength: UInt32?

	public init(secret: Data, digest: String, salt: Data, nonce: Data, payload: Data, filename: String? = nil, contentLength: UInt32? = nil) {
		self.secret = secret
		self.digest = digest
		self.salt = salt
		self.nonce = nonce
		self.payload = payload
        self.filename = filename
        self.contentLength = contentLength
	}
}
