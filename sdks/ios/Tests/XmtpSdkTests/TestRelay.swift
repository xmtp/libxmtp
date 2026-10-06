import Foundation
import Network

/// A loopback TCP relay to the backend, owned by one test. `disconnect` resets
/// every relayed connection and refuses new ones until `restore`. Other clients
/// on the backend are not affected.
final class TestRelay: @unchecked Sendable {
	private let queue = DispatchQueue(label: "org.xmtp.sdk.test-relay")
	private let listener: NWListener
	private let backend: NWEndpoint
	/// Read and changed only on `queue`.
	private var connections: [NWConnection] = []
	private var refusing = false

	init(backend url: String) throws {
		guard let parsed = URL(string: url), let host = parsed.host,
		      let port = parsed.port.flatMap({ NWEndpoint.Port(rawValue: UInt16($0)) })
		else { throw TestFailure("the backend URL \(url) has no host and port") }
		backend = .hostPort(host: NWEndpoint.Host(host), port: port)
		let parameters = NWParameters.tcp
		parameters.requiredLocalEndpoint = .hostPort(host: .ipv4(.loopback), port: .any)
		listener = try NWListener(using: parameters)
	}

	/// Starts the relay. Returns the URL that clients use in place of the backend URL.
	func start() async throws -> String {
		let settled = Shared(false)
		try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
			listener.stateUpdateHandler = { state in
				let result: Result<Void, Error>
				switch state {
				case .ready: result = .success(())
				case let .failed(error): result = .failure(error)
				default: return
				}
				var first = false
				settled.update { done in
					first = !done
					done = true
				}
				if first {
					continuation.resume(with: result)
				}
			}
			listener.newConnectionHandler = { [weak self] connection in
				self?.accept(connection)
			}
			listener.start(queue: queue)
		}
		guard let port = listener.port else { throw TestFailure("the relay has no port") }
		return "http://127.0.0.1:\(port.rawValue)"
	}

	/// Resets every relayed connection and refuses new ones.
	func disconnect() {
		queue.sync {
			refusing = true
			let open = connections
			connections = []
			for connection in open {
				connection.forceCancel()
			}
		}
	}

	/// Accepts and relays new connections again.
	func restore() {
		queue.sync { refusing = false }
	}

	func close() {
		queue.sync {
			listener.cancel()
			for connection in connections {
				connection.forceCancel()
			}
			connections = []
		}
	}

	private func accept(_ client: NWConnection) {
		if refusing {
			client.forceCancel()
			return
		}
		let server = NWConnection(to: backend, using: .tcp)
		connections += [client, server]
		for connection in [client, server] {
			connection.stateUpdateHandler = { [weak self] state in
				switch state {
				case .failed, .cancelled: self?.drop(client, server)
				default: break
				}
			}
			connection.start(queue: queue)
		}
		relay(from: client, to: server)
		relay(from: server, to: client)
	}

	private func relay(from source: NWConnection, to target: NWConnection) {
		source.receive(minimumIncompleteLength: 1, maximumLength: 65536) { [weak self] data, _, complete, error in
			guard let self else { return }
			if let data, !data.isEmpty {
				target.send(content: data, completion: .contentProcessed { [weak self] sendError in
					if sendError != nil {
						self?.drop(source, target)
					}
				})
			}
			if complete || error != nil {
				drop(source, target)
			} else {
				relay(from: source, to: target)
			}
		}
	}

	private func drop(_ first: NWConnection, _ second: NWConnection) {
		first.forceCancel()
		second.forceCancel()
		connections.removeAll { $0 === first || $0 === second }
	}
}
