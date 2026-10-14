@testable import ExampleCore
import Foundation
import Testing

@MainActor struct RoundSixteenAttachmentTests {
	private func row(_ id: String, published: Bool) -> ChatMessage {
		ChatMessage(id: id, conversationID: "chat", sender: "own", sentAt: 1,
		            delivery: published ? .published : .queued, content: .text("file"))
	}

	private func makeStore(_ fixture: AttachmentFixture, coordinator: SendCoordinator? = nil,
	                       admit: @escaping () -> Bool = { true },
	                       changed: @escaping () async -> Void = {}) -> AttachmentStore
	{
		AttachmentStore(gateway: fixture.gateway, drafts: fixture.drafts, coordinator: coordinator ?? fixture.coordinator,
		                conversationID: "chat",
		                origin: DestinationOrigin(session: SessionKey(profileID: UUID(), generation: 1), token: 1),
		                admit: { _ in admit() }, allowed: { true }, changed: changed)
	}

	private func acceptedDraft(_ fixture: AttachmentFixture) throws -> FileDraft {
		var draft = try fixture.add(phase: .complete)
		draft.acceptedID = "accepted"
		try fixture.drafts.save(draft)
		fixture.sends = [SendDraft(id: draft.id, conversationID: "chat", phase: .accepted, messageID: "accepted")]
		return draft
	}

	private func awaitGate(_ gate: ReviewGate) async throws {
		let deadline = ContinuousClock.now.advanced(by: .milliseconds(500))
		while !gate.started, ContinuousClock.now < deadline {
			try await Task.sleep(for: .milliseconds(1))
		}
	}

	@Test func publishedAttachmentIsRemovedAfterRouteCancellationWithoutUpdatingOldUI() async throws {
		let fixture = AttachmentFixture(); let draft = try acceptedDraft(fixture)
		var route = true; var published = false; var changed = 0
		let gate = ReviewGate()
		fixture.queries.publishHook = { published = true; route = false }
		fixture.queries.messageHook = { id in
			if published {
				await gate.wait(); try Task.checkCancellation()
			}
			return row(id, published: published)
		}
		let store = makeStore(fixture, admit: { route }, changed: { changed += 1 })
		store.send(.load); await store.wait()
		store.send(.send(draft.id))
		try await awaitGate(gate)
		#expect(gate.started)
		let closing = Task { await store.close() }
		for _ in 0 ..< 20 {
			await Task.yield()
		}
		gate.release(); await closing.value
		#expect(try fixture.drafts.all().isEmpty)
		#expect(fixture.sends.isEmpty)
		#expect(fixture.queries.publications == 1)
		#expect(fixture.queries.queues == 0)
		#expect(store.state.drafts.count == 1)
		#expect(changed == 0)
	}

	@Test func reopeningReconcilesAlreadyAcceptedPublishedAttachment() async throws {
		let fixture = AttachmentFixture(); _ = try acceptedDraft(fixture)
		fixture.sends = []
		fixture.queries.messageHook = { row($0, published: true) }
		let store = makeStore(fixture)
		store.send(.load); await store.wait()
		#expect(try fixture.drafts.all().isEmpty)
		#expect(store.state.drafts.isEmpty)
		#expect(fixture.queries.queues == 0)
		#expect(fixture.queries.publications == 0)
		await store.close()
	}

	@Test func acceptedPublicationFailureRetainsDraftUntilConfirmedRetry() async throws {
		let fixture = AttachmentFixture(); let draft = try fixture.add(phase: .complete)
		fixture.queries.publishHook = { throw ChatError.unknown }
		let store = makeStore(fixture)
		store.send(.send(draft.id)); await store.wait()
		#expect(try fixture.drafts.all().first?.acceptedID == "accepted")
		#expect(fixture.sends.first?.messageID == "accepted")
		store.send(.load); await store.wait()
		#expect(store.state.drafts.first?.acceptedID == "accepted")
		fixture.queries.publishHook = nil
		store.send(.send(draft.id)); await store.wait()
		#expect(try fixture.drafts.all().isEmpty)
		#expect(fixture.sends.isEmpty)
		#expect(fixture.gateway.uploads == 0)
		#expect(fixture.queries.queues == 1)
		#expect(fixture.queries.publications == 2)
		await store.close()
	}

	@Test func sessionReplacementAfterPublicationCannotRemoveAnotherProfilesDraft() async throws {
		let fixture = AttachmentFixture(); let draft = try acceptedDraft(fixture)
		var active = true; var published = false; var changed = 0
		var replacement = draft
		replacement.descriptor.filename = "replacement-profile"
		replacement.acceptedID = "replacement-operation"
		let coordinator = SendCoordinator(drafts: SendDraftStore(read: { fixture.sends }, write: { fixture.sends = $0 }),
		                                  queries: fixture.queries, accepts: { active })
		fixture.queries.messageHook = { row($0, published: published) }
		fixture.queries.publishHook = {
			published = true; active = false
			fixture.saved["attachment-" + draft.id.uuidString] = replacement
		}
		let store = makeStore(fixture, coordinator: coordinator, changed: { changed += 1 })
		store.send(.send(draft.id)); await store.wait()
		#expect(try fixture.drafts.all() == [replacement])
		#expect(fixture.refs == ["attachment-" + draft.id.uuidString])
		#expect(fixture.sends.first?.messageID == "accepted")
		#expect(store.state.drafts.isEmpty)
		#expect(changed == 0)
		await store.close()
	}

	@Test func confirmationChecksSessionBeforeAndAfterSDKRead() async throws {
		let fixture = AttachmentFixture(); let gate = ReviewGate()
		var active = false; var reads = 0
		let coordinator = SendCoordinator(drafts: SendDraftStore(read: { fixture.sends }, write: { fixture.sends = $0 }),
		                                  queries: fixture.queries, accepts: { active })
		fixture.queries.messageHook = { id in
			reads += 1
			if active {
				await gate.wait()
			}
			return row(id, published: true)
		}
		#expect(try await !coordinator.publicationConfirmed(conversationID: "chat", messageID: "accepted"))
		#expect(reads == 0)
		active = true
		let confirmation = Task { try await coordinator.publicationConfirmed(conversationID: "chat", messageID: "accepted") }
		try await awaitGate(gate)
		#expect(gate.started)
		active = false; gate.release()
		#expect(try await !confirmation.value)
		#expect(reads == 1)
	}

	@Test func loadDoesNotRemoveDraftChangedDuringConfirmation() async throws {
		let fixture = AttachmentFixture(); let draft = try acceptedDraft(fixture)
		var replacement = draft
		replacement.descriptor.filename = "changed-while-reading"
		fixture.queries.messageHook = { id in
			try fixture.drafts.save(replacement)
			return row(id, published: true)
		}
		let store = makeStore(fixture)
		store.send(.load); await store.wait()
		#expect(try fixture.drafts.all() == [replacement])
		#expect(store.state.drafts == [replacement])
		await store.close()
	}
}
