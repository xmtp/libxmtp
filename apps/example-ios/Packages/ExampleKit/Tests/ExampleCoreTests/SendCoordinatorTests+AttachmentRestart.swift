import ExampleCore
import Foundation
import Testing

extension SendCoordinatorTests {
	@Test func acceptedAttachmentRecoversAfterAcknowledgementFailureAndRestart() async throws {
		let fixture = AttachmentFixture(); let draft = try fixture.add(phase: .complete)
		var failAcknowledgement = true
		let drafts = AttachmentDraftStore(
			names: { Array(fixture.saved.keys) }, read: { fixture.saved[$0] },
			write: { key, value in
				if failAcknowledgement, value.acceptedID != nil {
					throw ChatError.storage
				}
				fixture.saved[key] = value
			},
			remove: { fixture.saved.removeValue(forKey: $0) },
			references: { fixture.refs }, saveReferences: { fixture.refs = $0 },
		)
		let origin = DestinationOrigin(session: SessionKey(profileID: UUID(), generation: 1), token: 1)
		let first = AttachmentStore(gateway: fixture.gateway, drafts: drafts, coordinator: fixture.coordinator,
		                            conversationID: "chat", origin: origin, admit: { _ in true }, allowed: { true })
		first.send(.send(draft.id)); await first.wait(); await first.close()
		#expect(fixture.sends.first?.phase == .accepted)
		#expect(fixture.sends.first?.messageID == "accepted")
		#expect(try drafts.all().first?.phase == .unknown)
		#expect(try drafts.all().first?.acceptedID == nil)
		#expect(fixture.queries.publications == 0)
		failAcknowledgement = false
		let restarted = SendCoordinator(drafts: SendDraftStore(read: { fixture.sends }, write: { fixture.sends = $0 }),
		                                queries: fixture.queries, accepts: { true })
		let recovered = AttachmentStore(gateway: fixture.gateway, drafts: drafts, coordinator: restarted,
		                                conversationID: "chat", origin: origin, admit: { _ in true }, allowed: { true })
		recovered.send(.load); await recovered.wait()
		#expect(try drafts.all().first?.acceptedID == "accepted")
		#expect(recovered.state.drafts.first?.phase == .complete)
		recovered.send(.send(draft.id)); await recovered.wait()
		#expect(fixture.gateway.uploads == 0)
		#expect(fixture.queries.queues == 1)
		#expect(fixture.queries.publications == 1)
		#expect(try drafts.all().isEmpty)
		#expect(fixture.sends.isEmpty)
		await recovered.close()
	}
}
