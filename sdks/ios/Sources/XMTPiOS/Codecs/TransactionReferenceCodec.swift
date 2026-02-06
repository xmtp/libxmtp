//
//  TransactionReferenceCodec.swift
//  XMTPiOS
//
//  Created by Naomi Plasterer on 8/1/25.
//

import Foundation

public let ContentTypeTransactionReference = ContentTypeID(
	authorityID: "xmtp.org",
	typeID: "transactionReference",
	versionMajor: 1,
	versionMinor: 0,
)

public struct TransactionReference {
	public struct Metadata {
		public let transactionType: String
		public let currency: String
		public let amount: Double
		public let decimals: UInt32
		public let fromAddress: String
		public let toAddress: String

		public init(
			transactionType: String,
			currency: String,
			amount: Double,
			decimals: UInt32,
			fromAddress: String,
			toAddress: String,
		) {
			self.transactionType = transactionType
			self.currency = currency
			self.amount = amount
			self.decimals = decimals
			self.fromAddress = fromAddress
			self.toAddress = toAddress
		}
	}

	public let namespace: String?
	public let networkId: String
	public let reference: String
	public let metadata: Metadata?

	public init(
		namespace: String? = nil,
		networkId: String,
		reference: String,
		metadata: Metadata? = nil,
	) {
		self.namespace = namespace
		self.networkId = networkId
		self.reference = reference
		self.metadata = metadata
	}
}

public struct TransactionReferenceCodec: ContentCodec {
	public typealias T = TransactionReference

	public init() {}

	public var contentType: ContentTypeID = ContentTypeTransactionReference

	public func encode(content: TransactionReference) throws -> EncodedContent {
		let ffi = FfiTransactionReference(
			namespace: content.namespace,
			networkId: content.networkId,
			reference: content.reference,
			metadata: content.metadata.map {
				FfiTransactionMetadata(
					transactionType: $0.transactionType,
					currency: $0.currency,
					amount: $0.amount,
					decimals: $0.decimals,
					fromAddress: $0.fromAddress,
					toAddress: $0.toAddress,
				)
			},
		)
		return try EncodedContent(
			serializedBytes: encodeTransactionReference(
				reference: ffi,
			),
		)
	}

	public func decode(content: EncodedContent) throws -> TransactionReference {
		let decoded = try decodeTransactionReference(
			bytes: content.serializedData(),
		)

		let metadata = decoded.metadata.map {
			TransactionReference.Metadata(
				transactionType: $0.transactionType,
				currency: $0.currency,
				amount: $0.amount,
				decimals: $0.decimals,
				fromAddress: $0.fromAddress,
				toAddress: $0.toAddress,
			)
		}

		return TransactionReference(
			namespace: decoded.namespace,
			networkId: decoded.networkId,
			reference: decoded.reference,
			metadata: metadata,
		)
	}

	public func fallback(content: TransactionReference) throws -> String? {
		"[Crypto transaction] Use a blockchain explorer to learn more using the transaction hash: \(content.reference)"
	}

	public func shouldPush(content _: TransactionReference) throws -> Bool {
		true
	}
}
