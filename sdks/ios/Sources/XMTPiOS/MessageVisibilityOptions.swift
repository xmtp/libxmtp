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

	/// Creates message visibility options
	/// - Parameter shouldPush: Whether this message should trigger a push notification (default: true)
	public init(shouldPush: Bool = true) {
		self.shouldPush = shouldPush
	}

	/// Converts the visibility options to FFI send message options
	/// - Returns: FfiSendMessageOpts instance with the appropriate settings
	public func toFfi() -> FfiSendMessageOpts {
		FfiSendMessageOpts(shouldPush: shouldPush)
	}
}
