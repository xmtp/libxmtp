import ExampleCore
import XmtpSdk

public struct SDKIdentityQueries: IdentityQueries {
	private let client: SDKClient
	private let limiter: AsyncLimiter
	public init(client: SDKClient, limiter: AsyncLimiter) {
		self.client = client; self.limiter = limiter
	}

	public func inboxID(for identity: IdentityInput) async throws -> String? {
		let value: PublicIdentity
		switch identity {
		case let .inbox(inboxID): return inboxID
		case let .ethereum(address): value = PublicIdentity(identifier: address, kind: .ethereum)
		case let .handle(handle): value = PublicIdentity(identifier: handle, kind: .handle)
		}
		do {
			guard let inboxID = try await limiter.withPermit({ [client] in
				try await client.inboxIdFor(identity: value)
			}) else { return nil }
			try Task.checkCancellation()
			if case let .ethereum(address) = identity {
				let states = try await limiter.withPermit { [client] in
					try await client.inboxStates(ids: [inboxID], refreshFromNetwork: true)
				}
				try Task.checkCancellation()
				guard states.contains(where: { state in
					state.inboxId == inboxID && state.identities.contains { member in
						member.kind == .ethereum && member.identifier.lowercased() == address.lowercased()
					}
				}) else { return nil }
			}
			return inboxID
		} catch {
			if lookupIsNotFound(error) {
				return nil
			}; throw error
		}
	}

	public func handles(inboxIDs: [String]) async throws -> [InboxIdentity] {
		try await limiter
			.withPermit { [client] in try await client.inboxStates(ids: inboxIDs, refreshFromNetwork: false).map { InboxIdentity(
				inboxID: $0.inboxId,
				handle: $0.handle,
				otherHandles: $0.otherHandles,
			) } }
	}
}

func lookupIsNotFound(_ error: any Error) -> Bool {
	if case XmtpError
		.IdentityNotFound = error
	{
		return true
	}; return false
}
