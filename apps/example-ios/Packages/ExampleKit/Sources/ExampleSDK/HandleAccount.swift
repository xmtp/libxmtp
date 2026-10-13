import ExampleCore
import Foundation
import XmtpSdk

@MainActor public final class SDKHandleAccount: HandleAccount {
	private let client: SDKClient
	private let limiter: AsyncLimiter
	public private(set) var claimTimes: [String: Date]
	private let persist: @MainActor (String, Date) throws -> Void
	public var minimum: Int {
		Int(client.serverConfiguration().handles.minNameLength)
	}

	public init(
		client: SDKClient,
		limiter: AsyncLimiter = AsyncLimiter(),
		claimTimes: [String: Date] = [:],
		recordClaim: @escaping @MainActor (String, Date) throws -> Void = { _, _ in },
	) {
		self.client = client; self.limiter = limiter; self.claimTimes = claimTimes; persist = recordClaim
	}

	public func handles() async throws -> OwnHandles {
		let value = try await limiter.withPermit { [client] in
			try await client.inboxState(refreshFromNetwork: false)
		}; return OwnHandles(
			current: value.handle,
			others: value.otherHandles,
		)
	}

	public func availableHandles(name: String) async throws
		-> [String]
	{
		do {
			return try await limiter.withPermit { [client] in
				try await client.availableHandles(name: name)
			}
		} catch { throw mapHandleError(error) }
	}

	public func claimHandle(handle: String) async throws {
		do { try await client.claimHandle(handle: handle) } catch { throw mapHandleError(error) }
	}

	public func recordClaim(handle: String, at date: Date) throws {
		try persist(handle, date); claimTimes[handle] = date
	}
}

func mapHandleError(_ error: any Error) -> HandleError {
	switch error {
	case XmtpError.HandleTaken: .taken
	case XmtpError.HandleNotAllowed: .notAllowed
	case XmtpError.HandleTooSoon: .tooSoon
	case XmtpError.HandleLimitReached: .limitReached
	case XmtpError.InvalidArgument: .invalidArgument
	default: .failed
	}
}
