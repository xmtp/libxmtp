//
//  SignatureRequest.swift
//  XMTPiOS
//
//  Created by Naomi Plasterer on 2/24/25.
//

import Foundation
import LibXMTP

public struct SignatureRequest {
	public let ffiSignatureRequest: FfiSignatureRequest

	public init(ffiSignatureRequest: FfiSignatureRequest) {
		self.ffiSignatureRequest = ffiSignatureRequest
	}

	public func addScwSignature(
		signatureBytes: Data,
		address: String,
		chainId: UInt64,
		blockNumber: UInt64? = nil
	) async throws {
		try await ffiSignatureRequest.addScwSignature(
			signatureBytes: signatureBytes,
			address: address,
			chainId: chainId,
			blockNumber: blockNumber
		)
	}

	public func addEcdsaSignature(signatureBytes: Data) async throws {
		try await ffiSignatureRequest.addEcdsaSignature(
			signatureBytes: signatureBytes)
	}

	public func signatureText() async throws -> String {
		return try await ffiSignatureRequest.signatureText()
	}
}
