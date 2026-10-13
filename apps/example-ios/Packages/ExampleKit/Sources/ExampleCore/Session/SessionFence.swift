import Foundation
import Observation

public struct SessionKey: Hashable, Sendable {
	public let profileID: UUID
	public let generation: UInt64
	public init(profileID: UUID, generation: UInt64) {
		self.profileID = profileID; self.generation = generation
	}
}

public struct DestinationOrigin: Sendable {
	public let session: SessionKey
	public let token: UInt64
	public init(session: SessionKey, token: UInt64) {
		self.session = session; self.token = token
	}
}

@MainActor public final class SessionFence {
	private var generation: UInt64 = 0
	private var profileID: UUID?
	private var explicitlyReserved = false
	public init() {}
	public func replace(_ profileID: UUID?) -> SessionKey? {
		_ = reserve(); self.profileID = profileID; return profileID.map { SessionKey(profileID: $0, generation: generation) }
	}

	public func reserve() -> UInt64 {
		explicitlyReserved = true; generation += 1; profileID = nil; return generation
	}

	public func reserveRestore() -> UInt64? {
		guard !explicitlyReserved else { return nil }; generation += 1; profileID = nil; return generation
	}

	public func currentGeneration() -> UInt64 {
		generation
	}

	public func reserveRestoreIfCurrent(_ expected: UInt64) -> UInt64? {
		guard generation == expected, profileID == nil else { return nil }; return reserveRestore()
	}

	public func releaseRestore(_ key: SessionKey) -> Bool {
		guard accepts(key) else { return false }; generation += 1; profileID = nil; return true
	}

	public func bind(_ profileID: UUID, reserved: UInt64) -> SessionKey? {
		guard isReserved(reserved) else { return nil }; self.profileID = profileID; return SessionKey(
			profileID: profileID,
			generation: generation,
		)
	}

	public func isReserved(_ reserved: UInt64) -> Bool {
		generation == reserved && profileID == nil
	}

	public func accepts(_ key: SessionKey) -> Bool {
		generation == key.generation && profileID == key.profileID
	}

	public func withCurrent<T>(_ key: SessionKey, _ body: () -> T) -> T? {
		guard accepts(key) else { return nil }; return body()
	}
}

@MainActor public protocol FeatureStore: AnyObject, Observable {
	associatedtype State: Equatable & Sendable
	associatedtype Action: Sendable
	var state: State { get }
	func send(_ action: Action)
}
