import ExampleCore
import XmtpSdk

@MainActor public final class SDKNewConversationQueries: NewConversationQueries {
	private let client: SDKClient
	private let identity: any IdentityQueries
	private let reads: AsyncLimiter
	public init(client: SDKClient, identity: any IdentityQueries, reads: AsyncLimiter) {
		self.client = client; self.identity = identity; self.reads = reads
	}

	public func maximumMembers() async throws -> Int {
		let config = client
			.serverConfiguration(); return Int(clamping: config.mls.maxGroupMembers)
	}

	public func resolve(_ input: String) async throws -> Recipient {
		let parsed: IdentityInput
		do { parsed = try IdentityInput.parse(input) } catch let error as IdentityError {
			throw NewConversationError.input(error.rawValue)
		}
		let id: String
		do { id = try await parsed.resolve(using: identity) } catch let error as IdentityError {
			throw NewConversationError.input(error.rawValue)
		}
		var handle: String?
		do { handle = try await identity.handles(inboxIDs: [id]).first?.handle } catch { handle = nil }
		return Recipient(inboxID: id, handle: handle)
	}

	public func people() async throws -> [Recipient] {
		let conversations = try await reads
			.withPermit { try await self.client.conversations.list(options: ListConversationsOptions(
				consentStates: [.allowed],
				limit: 20,
			)) }
		var people: [Recipient] = []
		for conversation in conversations.prefix(20) {
			let members = try await reads.withPermit { try await conversation.members() }
			for member in members.prefix(50)
				where member.inboxId != client.inboxId() && !people
				.contains(where: { $0.inboxID == member.inboxId })
			{
				people.append(Recipient(
					inboxID: member.inboxId,
					handle: member.handle,
				)); if people.count == 100 {
					return people
				}
			}
		}
		return people
	}

	public func create(recipients: [String], group: Bool, name: String, description: String,
	                   adminOnly: Bool) async throws -> String
	{
		if group {
			return try await client.conversations.createGroup(
				members: recipients,
				options: CreateGroupOptions(
					permissions: adminOnly ? .adminOnly : .allMembers,
					name: name,
					description: description,
				),
			).id()
		}
		guard recipients.count == 1,
		      let peer = recipients.first else { throw NewConversationError.input("A direct message needs one recipient.") }
		return try await client.conversations.createDm(peer: peer).id()
	}
}
