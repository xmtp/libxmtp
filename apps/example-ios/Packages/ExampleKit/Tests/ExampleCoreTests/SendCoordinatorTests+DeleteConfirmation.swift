import ExampleCore
import Foundation
import Testing

extension SendCoordinatorTests {
	@Test func deletionConfirmationKeepsTargetAfterAlertDismissal() async {
		let fixture = AttachmentFixture()
		let actions = MessageActionStore(queries: fixture.queries, coordinator: fixture.coordinator,
		                                 origin: DestinationOrigin(
		                                 	session: SessionKey(profileID: UUID(), generation: 1),
		                                 	token: 1,
		                                 ),
		                                 admit: { _ in true }, own: "own", changed: {})
		actions.allowed = true
		let row = ChatMessage(
			id: "delete-target",
			conversationID: "chat",
			sender: "own",
			sentAt: 1,
			content: .text("original"),
		)
		actions.send(.requestDelete(row)); actions.send(.confirmDelete)
		actions.send(.cancelDelete)
		await actions.wait()
		#expect(fixture.queries.deletes == ["delete-target"])
		#expect(actions.state.deleteTarget == nil)
		#expect(actions.state.error == nil)
	}
}
