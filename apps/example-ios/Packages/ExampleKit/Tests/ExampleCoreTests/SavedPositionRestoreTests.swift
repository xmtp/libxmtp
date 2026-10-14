@testable import ExampleCore
import Foundation
import Testing

@MainActor struct SavedPositionRestoreTests {
	private func queries(deleted: Bool = false) -> ChatQueryFake {
		let queries = ChatQueryFake()
		var rows: [ChatMessage] = []
		for value in (1 ... 125).reversed() {
			if deleted, value == 14 {
				continue
			}
			let position = HistoryPosition(sentAt: Int64(value), cursor: String(value))
			rows.append(ChatMessage(
				id: String(value),
				conversationID: "chat",
				sender: "peer",
				sentAt: Int64(value),
				content: .text("history-" + String(value)),
				position: position,
			))
		}
		queries.rows = rows
		return queries
	}

	private func redact(_ queries: ChatQueryFake, matching condition: (ChatMessage) -> Bool) {
		queries.rows = queries.rows.map { row in
			ChatMessage(
				id: row.id,
				conversationID: row.conversationID,
				sender: row.sender,
				sentAt: row.sentAt,
				content: condition(row) ? .deleted : row.content,
				position: row.position,
			)
		}
	}

	private var anchor: ScrollAnchor {
		ScrollAnchor(messageID: "14", position: HistoryPosition(sentAt: 14, cursor: "14"))
	}

	private var origin: DestinationOrigin {
		DestinationOrigin(
			session: SessionKey(profileID: UUID(), generation: 1),
			token: 1,
		)
	}

	@Test func savedReadingModeSurvivesInitialBottomVisibilityAndIncomingMessage() async {
		let queries = queries(); var saved: [ScrollAnchor] = []
		let timeline = ChatTimelineStore(
			id: "chat",
			origin: origin,
			queries: queries,
			admit: { _ in true },
			readAnchor: { anchor },
			saveAnchor: { saved.append($0) },
		)
		timeline.send(.load); await timeline.wait()
		#expect(timeline.state.target == "14")
		#expect(timeline.state.anchorID == "14")
		#expect(timeline.state.window.rows.contains { $0.id == "14" })
		#expect(!timeline.state.atNewest)
		timeline.send(.visibleMessages(["63", "64"], foreground: true))
		#expect(saved.isEmpty)
		#expect(timeline.state.target == "14")
		#expect(timeline.state.anchorID == "14")
		queries.rows.insert(
			ChatMessage(id: "126", conversationID: "chat", sender: "peer", sentAt: 126, content: .text("new"),
			            position: HistoryPosition(
			            	sentAt: 126,
			            	cursor: "126",
			            )),
			at: 0,
		)
		await timeline.refresh()
		#expect(timeline.state.window.rows.contains { $0.id == "14" })
		#expect(timeline.state.target == "14")
		timeline.send(.visibleMessages(["14", "15"], foreground: true))
		#expect(timeline.state.target == nil)
		#expect(saved.last?.messageID == "14")
		#expect(!timeline.state.atNewest)
	}

	@Test func deletedSavedRowRestoresNearestSurvivorInReadingMode() async {
		let timeline = ChatTimelineStore(
			id: "chat",
			origin: origin,
			queries: queries(deleted: true),
			admit: { _ in true },
			readAnchor: { anchor },
		)
		timeline.send(.load); await timeline.wait()
		#expect(timeline.state.window.notice == "Position changed")
		#expect(timeline.state.target == "15")
		#expect(timeline.state.anchorID == "15")
		#expect(timeline.state.window.rows.contains { $0.id == "15" })
		#expect(!timeline.state.atNewest)
	}

	@Test func explicitLatestStillAdmitsNewerMessagesAfterAbandoningRestore() async {
		let queries = queries()
		let timeline = ChatTimelineStore(
			id: "chat",
			origin: origin,
			queries: queries,
			admit: { _ in true },
			readAnchor: { anchor },
		)
		timeline.send(.load); await timeline.wait()
		timeline.send(.newest); await timeline.wait()
		#expect(timeline.state.target == nil)
		#expect(timeline.state.atNewest)
		queries.rows.insert(
			ChatMessage(id: "126", conversationID: "chat", sender: "peer", sentAt: 126, content: .text("new"),
			            position: HistoryPosition(
			            	sentAt: 126,
			            	cursor: "126",
			            )),
			at: 0,
		)
		await timeline.refresh()
		#expect(timeline.state.messages.last?.id == "126")
	}

	@Test func emptyInitialVisibilityPreservesLatestAppendMode() async {
		let queries = queries()
		let timeline = ChatTimelineStore(id: "chat", origin: origin, queries: queries, admit: { _ in true })
		timeline.send(.visibleMessages([], foreground: true))
		#expect(timeline.state.atNewest)
		timeline.send(.load); await timeline.wait()
		timeline.send(.visibleMessages([], foreground: true))
		#expect(timeline.state.atNewest)
		queries.rows.insert(
			ChatMessage(id: "126", conversationID: "chat", sender: "peer", sentAt: 126, content: .text("new"),
			            position: HistoryPosition(
			            	sentAt: 126,
			            	cursor: "126",
			            )),
			at: 0,
		)
		await timeline.refresh()
		#expect(timeline.state.messages.last?.id == "126")
	}

	@Test func committedRemoteTombstoneRefreshMovesToSurvivorAndKeepsNotice() async {
		let queries = queries(); var saved: [ScrollAnchor] = []
		let timeline = ChatTimelineStore(
			id: "chat",
			origin: origin,
			queries: queries,
			admit: { _ in true },
			readAnchor: { anchor },
			saveAnchor: { saved.append($0) },
		)
		timeline.send(.load); await timeline.wait()
		timeline.send(.visibleMessages(["14", "15"], foreground: true))
		#expect(timeline.state.target == nil)
		redact(queries) { $0.sentAt <= 60 }
		await timeline.refresh()
		#expect(timeline.state.window.rows.first { $0.id == "14" }?.content.isDeleted == true)
		#expect(timeline.state.window.notice == "Position changed")
		#expect(timeline.state.target == "61")
		#expect(timeline.state.anchorID == "61")
		#expect(!timeline.state.atNewest)
		timeline.send(.visibleMessages(["14", "15"], foreground: true))
		#expect(timeline.state.target == "61")
		#expect(timeline.state.anchorID == "61")
		#expect(saved.last?.messageID == "14")
		timeline.send(.visibleMessages(["61", "62"], foreground: true))
		#expect(timeline.state.target == nil)
		#expect(saved.last?.messageID == "61")
		timeline.send(.visibleMessages(["14"], foreground: true))
		#expect(timeline.state.anchorID == "61")
		#expect(saved.last?.messageID == "61")
		queries.rows.insert(
			ChatMessage(id: "126", conversationID: "chat", sender: "peer", sentAt: 126, content: .text("new"),
			            position: HistoryPosition(
			            	sentAt: 126,
			            	cursor: "126",
			            )),
			at: 0,
		)
		await timeline.refresh()
		#expect(timeline.state.window.notice == "Position changed")
		#expect(timeline.state.target == nil)
		#expect(timeline.state.anchorID == "61")
		#expect(!timeline.state.atNewest)
	}

	@Test func deletionRefreshWhileAtNewestKeepsFollowingNewerMessages() async {
		let queries = queries()
		let timeline = ChatTimelineStore(id: "chat", origin: origin, queries: queries, admit: { _ in true })
		timeline.send(.load); await timeline.wait()
		timeline.send(.visibleMessages((113 ... 125).map(String.init), foreground: true))
		#expect(timeline.state.atNewest)
		redact(queries) { $0.id == "113" }
		await timeline.refresh()
		#expect(timeline.state.window.notice == "Position changed")
		#expect(timeline.state.anchorID == "114")
		#expect(timeline.state.target == nil)
		#expect(timeline.state.atNewest)
		queries.rows.insert(
			ChatMessage(id: "126", conversationID: "chat", sender: "peer", sentAt: 126, content: .text("new"),
			            position: HistoryPosition(
			            	sentAt: 126,
			            	cursor: "126",
			            )),
			at: 0,
		)
		await timeline.refresh()
		#expect(timeline.state.messages.last?.id == "126")
		#expect(timeline.state.atNewest)
		#expect(timeline.state.target == nil)
	}
}
