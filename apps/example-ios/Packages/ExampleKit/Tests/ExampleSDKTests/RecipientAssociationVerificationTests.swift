import ExampleCore
@testable import ExampleSDK
import Foundation
import Testing
@testable import XmtpSdk

struct RecipientAssociationVerificationTests {
	@Test(arguments: [RecipientAssociationAnswer.matching, .wrongAddress, .wrongKind, .missingState, .wrongInbox])
	func addressResolutionRequiresItsVerifiedAssociation(answer: RecipientAssociationAnswer) async throws {
		let probe = AssociationProbe(answer: answer)
		let client = SDKClient(AssociationClient(probe: probe), codecs: [])
		let queries = SDKIdentityQueries(client: client, limiter: AsyncLimiter())
		let result = try await queries.inboxID(for: .ethereum(AssociationProbe.address))
		#expect(result == (answer == .matching ? AssociationProbe.inbox : nil))
		#expect(await probe.refreshes == [true])
		try await client.end()
	}

	@Test func verificationFailureRemainsAnError() async throws {
		let probe = AssociationProbe(answer: .failure)
		let client = SDKClient(AssociationClient(probe: probe), codecs: [])
		let queries = SDKIdentityQueries(client: client, limiter: AsyncLimiter())
		do {
			_ = try await queries.inboxID(for: .ethereum(AssociationProbe.address))
			Issue.record("Unverified backend mapping was accepted")
		} catch AssociationFailure.unavailable {} catch { throw error }
		#expect(await probe.refreshes == [true])
		try await client.end()
	}

	@Test func canceledLookupCannotStartAssociationRead() async throws {
		let probe = AssociationProbe(answer: .canceledLookup)
		let client = SDKClient(AssociationClient(probe: probe), codecs: [])
		let queries = SDKIdentityQueries(client: client, limiter: AsyncLimiter())
		let lookup = Task { try await queries.inboxID(for: .ethereum(AssociationProbe.address)) }
		do {
			_ = try await lookup.value
			Issue.record("Canceled recipient lookup was accepted")
		} catch is CancellationError {} catch { throw error }
		#expect(await probe.refreshes.isEmpty)
		try await client.end()
	}
}

enum RecipientAssociationAnswer: Sendable { case matching, wrongAddress, wrongKind, missingState, wrongInbox, failure,
                                                 canceledLookup
}

private enum AssociationFailure: Error { case unavailable }
private actor AssociationProbe {
	static let address = "0x" + String(repeating: "ab", count: 20)
	static let inbox = String(repeating: "1", count: 64)
	let answer: RecipientAssociationAnswer
	var refreshes: [Bool] = []
	init(answer: RecipientAssociationAnswer) {
		self.answer = answer
	}

	func states(refresh: Bool) throws -> [InboxState] {
		refreshes.append(refresh)
		if answer == .failure {
			throw AssociationFailure.unavailable
		}
		if answer == .missingState {
			return []
		}
		let identifier: String = answer == .wrongAddress
			? "0x" + String(repeating: "cd", count: 20) : Self.address.uppercased()
		let kind: PublicIdentityKind = answer == .wrongKind ? .handle : .ethereum
		let identity = PublicIdentity(identifier: identifier, kind: kind)
		let inboxID: String = answer == .wrongInbox ? String(repeating: "2", count: 64) : Self.inbox
		let state = InboxState(
			inboxId: inboxID,
			identities: [identity],
			installations: [],
			recoveryIdentity: identity,
			creationSignatureKind: nil,
			handle: nil,
			otherHandles: [],
		)
		return [state]
	}
}

private final class AssociationClient: Client, @unchecked Sendable {
	private let probe: AssociationProbe
	init(probe: AssociationProbe) {
		self.probe = probe; super.init(noHandle: NoHandle())
	}

	required init(unsafeFromHandle _: UInt64) {
		fatalError("Fake has no native handle")
	}

	override func clientKey() -> UInt64 {
		UInt64(truncatingIfNeeded: ObjectIdentifier(self).hashValue)
	}

	override func inboxIdFor(identity _: PublicIdentity) async throws -> InboxId? {
		if probe.answer == .canceledLookup {
			withUnsafeCurrentTask { $0?.cancel() }
		}
		return AssociationProbe.inbox
	}

	override func inboxStates(ids _: [InboxId], refreshFromNetwork: Bool) async throws -> [InboxState] {
		try await probe.states(refresh: refreshFromNetwork)
	}

	override func end() async throws {}
}
