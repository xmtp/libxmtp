import Foundation
import Observation

public enum RequestDecision: Equatable, Sendable { case delete(String), block(String) }
public enum RequestsAction: Sendable { case accept(String), requestDelete(String), requestBlock(String), confirm,
                                            cancel
}

public struct RequestsState: Equatable,
	Sendable { public var confirmation: RequestDecision?; public var busy = false; public var error: String? }
@MainActor @Observable public final class RequestsStore {
	public private(set) var state = RequestsState()
	private let session: SessionKey
	private let fence: SessionFence
	private let routes: RouteOwner
	private let queries: any ConversationQueries
	private let preferences: any ConversationPreferences
	private var job: Task<Void, Never>?
	private var inFlight: UUID?
	public init(
		session: SessionKey,
		fence: SessionFence,
		routes: RouteOwner,
		queries: any ConversationQueries,
		preferences: any ConversationPreferences,
	) {
		self.session = session; self.fence = fence; self.routes = routes; self.queries = queries; self
			.preferences = preferences
	}

	public func send(_ action: RequestsAction) {
		guard fence.accepts(session) else { return }
		if inFlight == nil {
			state.busy = false
		}
		switch action {
		case let .requestDelete(id): state.error = nil; state.confirmation = .delete(id)
		case let .requestBlock(id): state.error = nil; state.confirmation = .block(id)
		case .cancel: state.confirmation = nil
		case let .accept(id): decide(id: id, consent: .allowed)
		case .confirm: confirmDecision()
		}
	}

	private func confirmDecision() {
		guard let decision = state.confirmation else { return }
		state.confirmation = nil
		switch decision {
		case let .delete(id):
			do {
				try preferences.dismissRequest(id)
				state.error = nil
				routes.replace(.chats)
			} catch {
				state.error = "Cannot delete request. Try again."
			}
		case let .block(id):
			decide(id: id, consent: .denied)
		}
	}

	public func close() async {
		job?.cancel(); await job?.value
	}

	func waitForOperation() async {
		await job?.value
	}

	private func apply(_ origin: DestinationOrigin, _ body: (inout RequestsState) -> Void) {
		guard routes.admit(origin, fence: fence) else { return }; body(&state)
	}

	private func decide(id: String, consent: ConversationConsent) {
		guard inFlight == nil else { return }
		let operation = UUID()
		inFlight = operation
		state.busy = true
		state.error = nil
		let origin = DestinationOrigin(session: session, token: routes.token)
		job = Task { [weak self] in
			guard let self else { return }
			defer {
				if inFlight == operation {
					inFlight = nil
				}
			}
			do {
				try await queries.setConsent(id: id, consent: consent); apply(origin) { $0.busy = false; $0.error = nil }; if routes
					.admit(
						origin,
						fence: fence,
					)
				{
					if consent == .allowed {
						routes.setPath([.chat(id)])
					} else {
						routes.replace(.chats)
					}
				}
			} catch { apply(origin) { $0.busy = false; $0.error = "Cannot update conversation. Try again." } }
		}
	}
}
