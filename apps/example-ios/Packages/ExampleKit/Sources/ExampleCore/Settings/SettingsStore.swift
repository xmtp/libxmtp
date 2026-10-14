import Foundation
import Observation

public struct SettingsState: Equatable, Sendable {
	public var appearance: String
	public var busy = false
	public var confirmation: SettingsConfirmation?
	public var error: String?
	public init(appearance: String) {
		self.appearance = appearance
	}
}

public enum SettingsConfirmation: Equatable, Sendable { case signOut, reset }
public enum SettingsAction: Sendable { case appearance(String), signOut, reset, confirm, cancel }
@MainActor @Observable public final class SettingsStore: FeatureStore {
	public private(set) var state: SettingsState
	private let saveAppearance: @MainActor (String) -> Void
	private let signOut: @MainActor () async throws -> Void
	private let reset: @MainActor () async throws -> Void
	private let valid: @MainActor () -> Bool
	public init(
		appearance: String,
		valid: @escaping @MainActor () -> Bool,
		saveAppearance: @escaping @MainActor (String) -> Void,
		signOut: @escaping @MainActor () async throws -> Void,
		reset: @escaping @MainActor () async throws -> Void,
	) {
		state = SettingsState(appearance: appearance); self.valid = valid; self.saveAppearance = saveAppearance; self
			.signOut = signOut; self.reset = reset
	}

	public func send(_ action: SettingsAction) {
		switch action {
		case let .appearance(value): state.appearance = value; saveAppearance(value)
		case .signOut: state.confirmation = .signOut
		case .reset: state.confirmation = .reset
		case .cancel: state.confirmation = nil
		case .confirm:
			guard let choice = state.confirmation, !state.busy else { return }
			state.confirmation = nil; state.busy = true
			Task { do {
				if choice == .signOut {
					try await signOut()
				} else {
					try await reset()
				}; apply { $0.busy = false }
			} catch {
				apply {
					$0.busy = false; $0.error = choice == .reset ? "Local deletion is incomplete" : "Cannot sign out. Try again."
				}
			} }
		}
	}

	public func dismissConfirmation(_ choice: SettingsConfirmation) {
		guard state.confirmation == choice else { return }
		state.confirmation = nil
	}

	private func apply(_ change: (inout SettingsState) -> Void) {
		guard valid() else { return }; change(&state)
	}
}
