import Foundation

/// A change to a group's opaque `appData`, as observed after it was applied.
public struct AppDataChange: Sendable {
	/// The group whose `appData` changed.
	public let groupId: String
	/// Value before the change. `nil` when nothing was set.
	public let oldValue: String?
	/// Value after the change. `nil` when the field was cleared.
	public let newValue: String?

	init(ffi: FfiAppDataChange) {
		groupId = ffi.groupId.toHex
		oldValue = ffi.oldValue
		newValue = ffi.newValue
	}
}

/// Notified when a processed message changed a group's `appData`.
///
/// The handler is awaited before message processing continues, so a semantic
/// merge — including republishing via ``Group/updateAppData(appData:)`` — can
/// finish first. It fires for changes this client made as well as remote ones,
/// so the merge must be idempotent.
public protocol AppDataChangeHandler: Sendable {
	func onAppDataChanged(_ change: AppDataChange) async
}

/// Unstable: the set of group-change callbacks to register on a client.
///
/// Only ``appData`` exists today. This is one struct rather than a bare
/// callback so handlers for the other mutable fields (name, description,
/// image url, admin lists, permissions, disappearing settings) can be added as
/// further defaulted properties — additive for existing callers.
///
/// Registered at client creation via ``ClientOptions/unstableChangeCallbacks``,
/// because the changes it reports arrive from the stream and sync paths, where
/// no SDK call is on the stack to carry them.
///
/// > Warning: Pre-release. The shape of the payloads and of this struct may
/// > change without a major version bump until it graduates.
public struct UnstableChangeCallbacks: Sendable {
	public var appData: AppDataChangeHandler?

	public init(appData: AppDataChangeHandler? = nil) {
		self.appData = appData
	}

	/// `nil` when nothing is registered, so the core never pays for the
	/// before/after snapshot on the message-processing path.
	func toFfi() -> FfiUnstableChangeCallbacks? {
		guard let appData else { return nil }
		return FfiUnstableChangeCallbacks(
			appData: FfiAppDataChangeHandlerBridge(handler: appData)
		)
	}
}

/// Adapts the SDK-level ``AppDataChangeHandler`` to the generated FFI protocol,
/// keeping `FfiAppDataChange` out of the public surface.
final class FfiAppDataChangeHandlerBridge: FfiAppDataChangeCallback {
	private let handler: AppDataChangeHandler

	init(handler: AppDataChangeHandler) {
		self.handler = handler
	}

	func onAppDataChanged(change: FfiAppDataChange) async {
		await handler.onAppDataChanged(AppDataChange(ffi: change))
	}
}
