//
//  MessageVisibilityOptions.swift
//
//
//  Created by XMTP on 1/15/25.
//

import Foundation

/// Options that control the visibility and notification behavior of a message
public struct MessageVisibilityOptions {
	/// Whether this message should trigger a push notification
	public var shouldPush: Bool

	/// Optional idempotency key. Re-sending identical content with the same key
	/// produces the same message id and is deduplicated. Defaults to a timestamp.
	public var idempotencyKey: String?

	/// Creates message visibility options
	/// - Parameters:
	///   - shouldPush: Whether this message should trigger a push notification (default: true)
	///   - idempotencyKey: Optional idempotency key for deterministic, deduplicated sends
	public init(shouldPush: Bool = true, idempotencyKey: String? = nil) {
		self.shouldPush = shouldPush
		self.idempotencyKey = idempotencyKey
	}

	/// Converts the visibility options to FFI send message options
	/// - Returns: FfiSendMessageOpts instance with the appropriate settings
	public func toFfi() -> FfiSendMessageOpts {
		FfiSendMessageOpts(shouldPush: shouldPush, idempotencyKey: idempotencyKey)
	}
}
