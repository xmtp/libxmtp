import Foundation
import Observation

public struct BackendProbeResult: Sendable { public let requiresToken: Bool; public init(requiresToken: Bool) {
	self.requiresToken = requiresToken
} }
public protocol BackendProbe: Sendable { func probe(_ url: BackendURL) async throws -> BackendProbeResult }
public struct ConnectState: Equatable, Sendable {
	public var address: String
	public var token = ""
	public var serverName = ""
	public var checking = false
	public var ready = false
	public var joining = false
	public var requiresToken = false
	public var error: String?
	public var canJoin: Bool {
		!checking && !joining && !address.trimmingCharacters(in: .whitespacesAndNewlines)
			.isEmpty && (!requiresToken || !token.isEmpty) && BackendURL.parse(address) != nil
	}
}

public enum ConnectAction: Sendable { case appear, addressChanged(String), tokenChanged(String), nameChanged(String),
                                           join, retryProbe, retryRestore, resetFailedProfile, continueReset
}

@MainActor @Observable public final class ConnectStore {
	public private(set) var state: ConnectState
	public let session: SessionController
	private let probe: any BackendProbe
	private let owner: RouteOwner
	private var task: Task<Void, Never>?
	private var attempt: UInt64 = 0
	public init(
		probe: any BackendProbe,
		session: SessionController,
		owner: RouteOwner,
		defaultURL: String = ProcessInfo.processInfo.environment["XMTP_BACKEND_URL"] ?? "http://127.0.0.1:5050",
	) {
		self.probe = probe; self.session = session; self
			.owner = owner; state = ConnectState(address: defaultURL)
	}

	public func send(_ action: ConnectAction) {
		switch action {
		case .appear: state.joining = false; state.token = ""; scheduleProbe()
		case let .addressChanged(value): state.address = value; state.token = ""; state.requiresToken = false; scheduleProbe()
		case let .tokenChanged(value): state.token = value
		case let .nameChanged(value): state.serverName = value
		case .join: join()
		case .retryProbe: scheduleProbe()
		default: recover(action)
		}
	}

	private func recover(_ action: ConnectAction) {
		switch action {
		case .retryRestore: task = Task { await session.retryRestore() }
		case .resetFailedProfile: task = Task {
				do { try await session.reset() } catch { /* SessionController exposes incomplete deletion. */ }
			}
		case .continueReset: task = Task {
				do { try await session.continueReset() } catch { /* SessionController exposes the resumable deletion state. */ }
			}
		default: break
		}
	}

	private func join() {
		guard state.canJoin, let url = BackendURL.parse(state.address) else { return }
		let token = state.token; let name = state.serverName; state.joining = true; state.error = nil; task?
			.cancel(); attempt += 1
		let expected = attempt; let route = owner.token; let initialGeneration = session.fence.currentGeneration()
		task = Task {
			var generation = initialGeneration
			guard expected == attempt, route == owner.token, generation == session.fence.currentGeneration(),
			      !Task.isCancelled else { return }
			do { try await session.connect(
				url: url.value,
				token: token,
				name: name.isEmpty ? nil : name,
				onReserved: { generation = $0 },
			)
			if expected == attempt {
				state.token = ""
			}
			apply(expected, route: route, generation: generation, url: url) { $0.joining = false }
			} catch {
				apply(expected, route: route, generation: generation, url: url) {
					$0.error = error.localizedDescription; $0.joining = false
				}
			}
		}
	}

	private func scheduleProbe() {
		task?.cancel(); attempt += 1; state.error = nil; state.ready = false; state.joining = false
		guard let url = BackendURL.parse(state.address) else { state.checking = false; return }
		state.checking = true; let expected = attempt; let route = owner.token; let generation = session.fence
			.currentGeneration()
		task = Task {
			do { try await Task.sleep(for: .milliseconds(300)); let result = try await probe.probe(url)
				apply(expected, route: route, generation: generation, url: url) {
					$0.checking = false; $0.requiresToken = result.requiresToken; $0.ready = true
				}
			} catch is CancellationError {} catch {
				apply(expected, route: route, generation: generation, url: url) {
					$0.checking = false; $0.error = "Cannot reach this server. Try again."
				}
			}
		}
	}

	private func apply(
		_ expected: UInt64,
		route: UInt64,
		generation: UInt64,
		url: BackendURL,
		change: (inout ConnectState) -> Void,
	) {
		guard expected == attempt, route == owner.token, generation == session.fence.currentGeneration(),
		      BackendURL.parse(state.address) == url else { return }; change(&state)
	}
}
