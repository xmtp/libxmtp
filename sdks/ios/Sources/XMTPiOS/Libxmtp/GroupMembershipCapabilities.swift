import Foundation

/// An MLS extension type advertised by an installation's key package or
/// present in a group's context.
///
/// Mirrors the libxmtp `MlsExtensionType`; forward/unknown types are
/// preserved verbatim. This is a generic capability primitive — filter it to
/// the question you care about (e.g. the proposal migration is concerned with
/// ``appDataDictionary``).
public enum MlsExtensionType: Equatable {
	case applicationId
	case ratchetTree
	case requiredCapabilities
	case externalPub
	case externalSenders
	case lastResort
	case immutableMetadata
	case appDataDictionary
	case unknown(id: UInt16)
	case grease(id: UInt16)

	init(ffi: FfiMlsExtensionType) {
		switch ffi {
		case .applicationId: self = .applicationId
		case .ratchetTree: self = .ratchetTree
		case .requiredCapabilities: self = .requiredCapabilities
		case .externalPub: self = .externalPub
		case .externalSenders: self = .externalSenders
		case .lastResort: self = .lastResort
		case .immutableMetadata: self = .immutableMetadata
		case .appDataDictionary: self = .appDataDictionary
		case let .unknown(id): self = .unknown(id: id)
		case let .grease(id): self = .grease(id: id)
		}
	}
}

/// Capabilities for a single installation (device) of a member.
public struct InstallationCapabilities {
	let ffi: FfiInstallationCapabilities

	public init(ffi: FfiInstallationCapabilities) {
		self.ffi = ffi
	}

	/// The installation (device) key.
	public var installationId: Data {
		ffi.installationId
	}

	/// True for the local (this device's) installation.
	public var isOwn: Bool {
		ffi.isOwn
	}

	/// The MLS extension types this installation advertises. Empty when
	/// ``capabilitiesKnown`` is false.
	public var supportedExtensions: [MlsExtensionType] {
		ffi.supportedExtensions.map { MlsExtensionType(ffi: $0) }
	}

	/// Whether capabilities were determined. `false` means the key package
	/// couldn't be fetched or failed verification — distinct from an
	/// installation that advertises no extensions.
	public var capabilitiesKnown: Bool {
		ffi.capabilitiesKnown
	}
}

/// Per-inbox installation capabilities. Map ``inboxId`` to a profile to
/// attribute capabilities to a person.
public struct InboxCapabilities {
	let ffi: FfiInboxCapabilities

	public init(ffi: FfiInboxCapabilities) {
		self.ffi = ffi
	}

	public var inboxId: String {
		ffi.inboxId
	}

	public var installations: [InstallationCapabilities] {
		ffi.installations.map { InstallationCapabilities(ffi: $0) }
	}
}

/// A generic membership/capability snapshot for a group.
///
/// Reports raw facts rather than answers. For the proposal
/// (app-data-dictionary) migration specifically: the group is already
/// migrated when ``contextExtensions`` contains ``MlsExtensionType/appDataDictionary``,
/// it's eligible to migrate when every installation's
/// ``InstallationCapabilities/supportedExtensions`` contains it, and the
/// inboxes blocking migration are those with an installation that doesn't.
///
/// Read it with ``Group/membershipCapabilities()``; drive the upgrade with
/// ``Group/enableProposals(force:minVersion:)``.
public struct GroupMembershipCapabilities {
	let ffi: FfiGroupMembershipCapabilities

	public init(ffi: FfiGroupMembershipCapabilities) {
		self.ffi = ffi
	}

	/// Extension types present in the group's context.
	public var contextExtensions: [MlsExtensionType] {
		ffi.contextExtensions.map { MlsExtensionType(ffi: $0) }
	}

	/// Per-inbox, per-installation capability breakdown — one entry per member
	/// inbox, in no particular order.
	public var members: [InboxCapabilities] {
		ffi.members.map { InboxCapabilities(ffi: $0) }
	}
}
