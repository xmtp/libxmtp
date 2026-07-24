import Foundation

/// Pre-release ("unstable") surface for a ``Group``, reached through
/// `group.unstable` and gated behind `@_spi(Unstable)`.
///
/// Everything here is unstable: the API shape may still change and, in
/// some cases (see ``enableProposals(force:minVersion:)``), the effect is
/// one-way and irreversible. Adding a function here needs no per-function
/// annotation — the `@_spi(Unstable)` gate on `Group.unstable` covers the
/// whole type. When an API graduates it moves onto ``Group`` directly and
/// is removed here, so callers of the `unstable` form get a compile-time
/// break to migrate against.
@_spi(Unstable) public struct UnstableGroup {
	let ffiGroup: FfiConversation

	/// Migrate this group's metadata from the legacy GroupContextExtensions
	/// shape onto OpenMLS `AppDataUpdate` proposals. After this returns
	/// successfully, subsequent metadata writes (group name, description,
	/// image URL, admin list, permissions) flow through the proposal-based
	/// path instead of GCE commits.
	///
	/// - Parameters:
	///   - force: Skip the pre-flight key-package capability check.
	///     Post-d14n every client supports proposals by version floor
	///     alone, so the per-member scan stops adding signal. Set
	///     `true` to bypass it. Callers using this MUST be confident
	///     every member is at `>= minVersion`. Defaults to `false`.
	///   - minVersion: Override the `MIN_SUPPORTED_PROTOCOL_VERSION`
	///     floor. `nil` defaults to libxmtp's
	///     `PROPOSALS_MIN_PROTOCOL_VERSION` — the release where
	///     proposals support first ships.
	///
	/// Hard-fails with `ProposalsNotSupported` if `force == false` and
	/// any member's latest key package doesn't advertise
	/// `ProposalType::AppDataUpdate`. The migration is one-way — a
	/// migrated group cannot return to the legacy path.
	public func enableProposals(force: Bool = false, minVersion: String? = nil) async throws {
		try await ffiGroup.enableProposals(
			options: FfiEnableProposalsOptions(force: force, minVersion: minVersion)
		)
	}
}
