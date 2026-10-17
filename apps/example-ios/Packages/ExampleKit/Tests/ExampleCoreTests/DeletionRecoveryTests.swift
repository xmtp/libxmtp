@testable import ExampleCore
import Foundation
import Testing

private enum DeletionUnknownFailure: Error { case interrupted }

@MainActor struct RoundFifteenDeletionRecoveryTests {
	private func message(_ id: String, conversation: String = "chat", delivery: MessageDelivery = .queued) -> ChatMessage {
		ChatMessage(
			id: id,
			conversationID: conversation,
			sender: "own",
			sentAt: 1,
			delivery: delivery,
			content: .text("kept"),
		)
	}

	@Test func deletionPersistsBeforeAcceptanceAndPublishesOperationInActualConversation() async throws {
		var records: [SendDraft] = []
		let queries = ChatQueryFake()
		queries.messageHook = { id in message(id, conversation: "source-group") }
		let store = SendDraftStore(read: { records }, write: { records = $0 })
		let coordinator = SendCoordinator(drafts: store, queries: queries, accepts: { true })
		queries.beforeDelete = {
			#expect(records.count == 1)
			#expect(records.first?.phase == .queueing)
			#expect(records.first?.deletionTargetID == "original-target")
			#expect(records.first?.messageID == nil)
		}
		let id = try await coordinator.delete(
			conversationID: "source-group",
			messageID: "original-target",
			admission: { true },
		)
		#expect(id == "deletion-operation")
		#expect(queries.deletes == ["original-target"])
		#expect(queries.queues == 0)
		#expect(queries.publishedIDs == ["deletion-operation"])
		#expect(queries.publicationConversations == ["source-group"])
		#expect(records.isEmpty)
	}

	@Test func knownPreacceptanceRejectionRestoresDeletionJournal() async {
		var records: [SendDraft] = []
		let queries = ChatQueryFake()
		queries.beforeDelete = { throw ChatError.rejected }
		let coordinator = SendCoordinator(drafts: SendDraftStore(read: { records }, write: { records = $0 }),
		                                  queries: queries, accepts: { true })
		await #expect(throws: ChatError.rejected) {
			try await coordinator.delete(conversationID: "chat", messageID: "original-target", admission: { true })
		}
		#expect(records.isEmpty)
		#expect(queries.deletes.isEmpty)
		#expect(queries.publishedIDs.isEmpty)
	}

	@Test func acceptedPublicationFailureDismissesActionAndKeepsRecoverableOperation() async throws {
		var records: [SendDraft] = []
		let queries = ChatQueryFake()
		queries.publishHook = { throw DeletionUnknownFailure.interrupted }
		let coordinator = SendCoordinator(drafts: SendDraftStore(read: { records }, write: { records = $0 }),
		                                  queries: queries, accepts: { true })
		let actions = MessageActionStore(queries: queries, coordinator: coordinator,
		                                 origin: DestinationOrigin(
		                                 	session: SessionKey(profileID: UUID(), generation: 1),
		                                 	token: 1,
		                                 ),
		                                 admit: { _ in true }, own: "own", changed: {})
		actions.allowed = true
		actions.send(.requestDelete(message("original-target", delivery: .published)))
		actions.send(.confirmDelete)
		await actions.wait()
		let saved = try #require(records.first)
		#expect(saved.phase == .accepted)
		#expect(saved.messageID == "deletion-operation")
		#expect(saved.deletionTargetID == "original-target")
		#expect(actions.state.deleteTarget == nil)
		#expect(actions.state.error == "Deletion needs review in Settings.")
		#expect(queries.deleteAttempts == 1)
		#expect(queries.publishedIDs == ["deletion-operation"])
	}

	@Test func restartedDeletionRetryPublishesAcceptedIDWithoutDeletingAgain() async throws {
		var disk = Data("[]".utf8)
		let queries = ChatQueryFake()
		var failPublication = true
		queries.publishHook = {
			if failPublication {
				throw DeletionUnknownFailure.interrupted
			}
		}
		let store = SendDraftStore(read: { try JSONDecoder().decode([SendDraft].self, from: disk) },
		                           write: { disk = try JSONEncoder().encode($0) })
		let first = SendCoordinator(drafts: store, queries: queries, accepts: { true })
		let original = try await first.delete(conversationID: "chat", messageID: "original-target", admission: { true })
		#expect(try store.all().first?.phase == .accepted)
		failPublication = false
		let reopenedStore = SendDraftStore(read: { try JSONDecoder().decode([SendDraft].self, from: disk) },
		                                   write: { disk = try JSONEncoder().encode($0) })
		let reopened = SendCoordinator(drafts: reopenedStore, queries: queries, accepts: { true })
		let retried = try await reopened.delete(conversationID: "chat", messageID: "original-target", admission: { true })
		#expect(retried == original)
		#expect(queries.deleteAttempts == 1)
		#expect(queries.publishedIDs == ["deletion-operation", "deletion-operation"])
		#expect(try reopenedStore.all().isEmpty)
	}

	@Test func failedAcceptedIDWriteRetainsDeletionAndDoesNotQueueItAgain() async throws {
		var records: [SendDraft] = []
		var failAcceptedWrite = true
		let queries = ChatQueryFake()
		let store = SendDraftStore(read: { records }, write: {
			if failAcceptedWrite, $0.contains(where: { $0.phase == .accepted }) {
				throw ChatError.storage
			}
			records = $0
		})
		let coordinator = SendCoordinator(drafts: store, queries: queries, accepts: { true })
		let id = try await coordinator.delete(conversationID: "chat", messageID: "original-target", admission: { true })
		#expect(id == "deletion-operation")
		#expect(records.first?.phase == .queueing)
		#expect(queries.publishedIDs.isEmpty)
		failAcceptedWrite = false
		_ = try await coordinator.delete(conversationID: "chat", messageID: "original-target", admission: { true })
		#expect(queries.deleteAttempts == 1)
		#expect(queries.publishedIDs == ["deletion-operation"])
		#expect(records.isEmpty)
	}

	@Test func unknownDeletionCannotBeQueuedAgainOrClearedByComposerReview() async throws {
		var records: [SendDraft] = []
		let queries = ChatQueryFake()
		queries.beforeDelete = { throw DeletionUnknownFailure.interrupted }
		let store = SendDraftStore(read: { records }, write: { records = $0 })
		let coordinator = SendCoordinator(drafts: store, queries: queries, accepts: { true })
		await #expect(throws: DeletionUnknownFailure.self) {
			try await coordinator.delete(conversationID: "chat", messageID: "original-target", admission: { true })
		}
		await #expect(throws: ChatError.duplicate) {
			try await coordinator.delete(conversationID: "chat", messageID: "original-target", admission: { true })
		}
		#expect(queries.deleteAttempts == 1)
		#expect(records.first?.deletionTargetID == "original-target")
		var composerDraft = ComposerDraft()
		composerDraft.edit("new text")
		let composer = ComposerStore(id: "chat",
		                             origin: DestinationOrigin(
		                             	session: SessionKey(profileID: UUID(), generation: 1),
		                             	token: 1,
		                             ),
		                             drafts: ComposerDraftStore(
		                             	read: { _ in composerDraft },
		                             	write: { _, value in composerDraft = value },
		                             ),
		                             sends: store, coordinator: coordinator, admit: { _ in true })
		#expect(!composer.state.unknown)
		composer.send(.clearReview)
		#expect(records.count == 1)
		#expect(composerDraft.text == "new text")
	}
}
