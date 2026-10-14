import Foundation
import Observation

public struct MetadataState: Equatable, Sendable { public var snapshot = MetadataSnapshot()
	public var busy = false
	public var error: String?
	public init() {}
}

@MainActor @Observable public final class MetadataStore {
	public private(set) var state = MetadataState()
	private let queries: any MetadataQueries
	private let origin: DestinationOrigin
	private let admit: @MainActor (DestinationOrigin) -> Bool
	private let run: @MainActor (@escaping @MainActor @Sendable () async -> Void) -> Void
	public enum Action: Sendable {
		case refresh, edit(MetadataEdit)
	}

	public init(
		queries: any MetadataQueries,
		origin: DestinationOrigin,
		admit: @escaping @MainActor (DestinationOrigin) -> Bool,
		run: @escaping @MainActor (@escaping @MainActor @Sendable () async -> Void) -> Void,
	) {
		self.queries = queries
		self.origin = origin
		self.admit = admit
		self.run = run
	}

	private func apply(_ block: (inout MetadataState) -> Void) {
		guard admit(origin) else { return }
		block(&state)
	}

	public func send(_ action: Action) {
		guard !state.busy, admit(origin) else { return }
		state.busy = true
		state.error = nil
		let baseline = state.snapshot
		run { [weak self] in
			guard let self else { return }
			do {
				if case let .edit(edit) = action {
					try await queries.edit(edit, baseline: baseline)
				}
				let snapshot = try await queries.load()
				apply { $0.snapshot = snapshot }
			} catch is CancellationError { } catch {
				do { let snapshot = try await queries.load()
					apply { $0.snapshot = snapshot }
				} catch {}
				apply { $0.error = "Cannot update these fields. Reload and try again." }
			}
			apply { $0.busy = false }
		}
	}
}
