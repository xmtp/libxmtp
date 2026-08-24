import Foundation

/// How thorough a ``Client/dbIntegrityCheck(level:)`` /
/// ``Client/checkDatabaseIntegrity(dbPath:encryptionKey:level:)`` run should
/// be.
public enum IntegrityCheckLevel: Sendable {
	/// Cheap structural checks (page/index consistency), safe to run
	/// frequently.
	case quick
	/// Exhaustive check of every row and index entry. Slower; reserve for
	/// diagnostics.
	case full

	func toFfi() -> FfiIntegrityCheckLevel {
		switch self {
		case .quick:
			.quick
		case .full:
			.full
		}
	}
}

/// Result of a ``Client/dbIntegrityCheck(level:)`` /
/// ``Client/checkDatabaseIntegrity(dbPath:encryptionKey:level:)`` run.
///
/// ``outcome`` is one of `"ok"`, `"corrupt"`, `"unreadable"`,
/// `"saltMissing"`, `"locked"`, or `"failed"`. ``findings`` holds row-level
/// findings for a `"corrupt"` outcome, or the error/reason string for other
/// non-`"ok"` outcomes; it is empty when ``outcome`` is `"ok"`.
public struct IntegrityCheckOutcome: Sendable {
	public let outcome: String
	public let findings: [String]

	init(_ ffi: FfiIntegrityCheckOutcome) {
		outcome = ffi.outcome
		findings = ffi.findings
	}
}
