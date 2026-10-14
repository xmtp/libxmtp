@testable import ExampleCore
import Foundation
import Testing

@MainActor struct RoundSeventeenAttachmentTests {
	private func store(_ fixture: AttachmentFixture, coordinator: SendCoordinator? = nil,
	                   admit: @escaping () -> Bool = { true }) -> AttachmentStore
	{
		AttachmentStore(gateway: fixture.gateway, drafts: fixture.drafts, coordinator: coordinator ?? fixture.coordinator,
		                conversationID: "chat",
		                origin: DestinationOrigin(session: SessionKey(profileID: UUID(), generation: 1), token: 1),
		                admit: { _ in admit() }, allowed: { true })
	}

	@Test(arguments: [ChatError.rejected, .unavailable, .stale], [FilePhase.waiting, .complete])
	func definiteRejectionRestoresUploadedDraftAndAllowsRetryWithoutUpload(
		error: ChatError,
		phase: FilePhase,
	) async throws {
		let fixture = AttachmentFixture(); let draft = try fixture.add(phase: phase)
		var attempts = 0
		fixture.queries.beforeQueue = {
			attempts += 1
			#expect(try fixture.drafts.all().first?.phase == .unknown)
			throw error
		}
		let attachments = store(fixture)
		attachments.send(.send(draft.id)); await attachments.wait()
		#expect(try fixture.drafts.all().first?.phase == .complete)
		#expect(try fixture.drafts.all().first?.acceptedID == nil)
		#expect(fixture.sends.isEmpty)
		#expect(attempts == 1)
		let uploads = fixture.gateway.uploads
		fixture.queries.beforeQueue = { attempts += 1 }
		attachments.send(.send(draft.id)); await attachments.wait()
		#expect(attempts == 2)
		#expect(fixture.gateway.uploads == uploads)
		#expect(fixture.queries.queues == 1)
		#expect(fixture.queries.publications == 1)
		#expect(try fixture.drafts.all().isEmpty)
		await attachments.close()
	}

	@Test(arguments: [ChatError.unknown, .storage])
	func ambiguousAcceptanceRemainsUnknownAndCannotBeQueuedAgain(error: ChatError) async throws {
		let fixture = AttachmentFixture(); let draft = try fixture.add(phase: .complete)
		var attempts = 0
		fixture.queries.beforeQueue = { attempts += 1; throw error }
		let attachments = store(fixture)
		attachments.send(.send(draft.id)); await attachments.wait()
		#expect(try fixture.drafts.all().first?.phase == .unknown)
		#expect(fixture.sends.first?.phase == .queueing)
		#expect(fixture.sends.first?.messageID == nil)
		attachments.send(.send(draft.id)); await attachments.wait()
		#expect(attempts == 1)
		#expect(fixture.queries.queues == 0)
		#expect(fixture.queries.publications == 0)
		await attachments.close()
	}

	@Test func definiteRejectionRestoresDurablePhaseAfterRouteBecomesStale() async throws {
		let fixture = AttachmentFixture(); let draft = try fixture.add(phase: .complete)
		var admitted = true
		fixture.queries.beforeQueue = { admitted = false; throw ChatError.stale }
		let attachments = store(fixture, admit: { admitted })
		attachments.send(.send(draft.id)); await attachments.wait()
		#expect(try fixture.drafts.all().first?.phase == .complete)
		#expect(fixture.sends.isEmpty)
		#expect(attachments.state.error == nil)
		await attachments.close()
	}

	@Test(arguments: [false, true])
	func rejectionDoesNotOverwriteChangedDraftOrReplacementProfile(replaceSession: Bool) async throws {
		let fixture = AttachmentFixture(); let draft = try fixture.add(phase: .complete)
		var active = true
		var replacement = draft
		replacement.phase = .failed
		replacement.descriptor.filename = "newer-draft"
		let coordinator = SendCoordinator(drafts: SendDraftStore(read: { fixture.sends }, write: { fixture.sends = $0 }),
		                                  queries: fixture.queries, accepts: { active })
		fixture.queries.beforeQueue = {
			if replaceSession {
				active = false
			}
			try fixture.drafts.save(replacement)
			throw ChatError.rejected
		}
		let attachments = store(fixture, coordinator: coordinator)
		attachments.send(.send(draft.id)); await attachments.wait()
		#expect(try fixture.drafts.all() == [replacement])
		await attachments.close()
	}
}
