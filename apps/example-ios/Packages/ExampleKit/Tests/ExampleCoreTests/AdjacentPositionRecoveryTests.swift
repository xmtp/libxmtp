@testable import ExampleCore
import Foundation
import Testing

@MainActor struct AdjacentPositionRecoveryTests {
	private var origin: DestinationOrigin {
		DestinationOrigin(session: SessionKey(profileID: UUID(), generation: 1), token: 1)
	}

	private var anchor: ScrollAnchor {
		ScrollAnchor(messageID: "9", position: HistoryPosition(sentAt: 9, cursor: "9"))
	}

	private func queries(count: Int = 125, content: (Int) -> ChatContent) -> ChatQueryFake {
		let queries = ChatQueryFake()
		var rows: [ChatMessage] = []
		for value in stride(from: count, through: 1, by: -1) {
			let messageID = String(value)
			let marker = Int64(value)
			let body: ChatContent = content(value)
			let position = HistoryPosition(sentAt: marker, cursor: messageID)
			let row = ChatMessage(
				id: messageID,
				conversationID: "chat",
				sender: "peer",
				sentAt: marker,
				insertedAt: marker,
				content: body,
				position: position,
			)
			rows.append(row)
		}
		queries.rows = rows
		return queries
	}

	private func store(_ queries: ChatQueryFake) -> ChatTimelineStore {
		ChatTimelineStore(id: "chat", origin: origin, queries: queries, admit: { _ in true }, readAnchor: { anchor })
	}

	@Test func bulkDeletionSeeksFirstSurvivingVisibleRow() async {
		let queries = queries { $0 <= 60 ? .deleted : .text("history-" + String($0)) }
		let timeline = store(queries)
		timeline.send(.load); await timeline.wait()
		#expect(timeline.state.target == "61")
		#expect(timeline.state.anchorID == "61")
		#expect(timeline.state.messages.contains { $0.id == "61" })
		#expect(timeline.state.window.notice == "Position changed")
		#expect(timeline.state.error == nil)
		#expect(queries.beforeRequests.count == 4)
	}

	@Test func reactionsCannotBecomeRestorationTargetsOrBlockReadingMarkers() async {
		let queries = queries { $0 <= 9 ? .deleted : ($0 == 10 ? .reaction : .text("history-" + String($0))) }
		var saved: [ScrollAnchor] = []
		let timeline = ChatTimelineStore(id: "chat", origin: origin, queries: queries, admit: { _ in true },
		                                 readAnchor: { anchor }, saveAnchor: { saved.append($0) })
		timeline.send(.load); await timeline.wait()
		#expect(timeline.state.target == "11")
		#expect(timeline.state.messages.contains { $0.id == timeline.state.target })
		timeline.send(.visibleMessages(["11", "12"], foreground: true))
		#expect(timeline.state.target == nil)
		#expect(saved.last?.messageID == "11")
	}

	@Test func retryContinuesOriginalAnchorAcrossBoundedWindows() async {
		let queries = queries(count: 180) { $0 <= 140 ? .deleted : .text("history-" + String($0)) }
		let timeline = store(queries)
		timeline.send(.load); await timeline.wait()
		#expect(timeline.state.target == nil)
		#expect(timeline.state.error == "More messages are needed to restore your position. Retry to continue.")
		#expect(queries.beforeRequests.count == 4)
		await timeline.refresh()
		#expect(timeline.state.target == "141")
		#expect(timeline.state.anchorID == "141")
		#expect(timeline.state.messages.contains { $0.id == "141" })
		#expect(timeline.state.error == nil)
		#expect(queries.beforeRequests.count == 6)
	}

	@Test func exhaustedDeletedHistoryIsReadableAndRetryRemainsBounded() async {
		let queries = queries { _ in .deleted }
		let timeline = store(queries)
		timeline.send(.load); await timeline.wait()
		await timeline.refresh()
		#expect(timeline.state.target == nil)
		#expect(timeline.state.error == "No surviving messages are available.")
		#expect(queries.beforeRequests.count <= 8)
		timeline.send(.visibleMessages(["120"], foreground: true))
		#expect(timeline.state.anchorID == nil)
		await timeline.refresh()
		#expect(timeline.state.error != "Message updates stopped.")
		#expect(timeline.state.target == nil)
		#expect(queries.beforeRequests.count <= 12)
	}

	@Test func allReactionPagesContinueToRenderableRow() async {
		let queries = queries { $0 == 120 ? .text("visible") : .reaction }
		var saved: [ScrollAnchor] = []; var markers: [Int64] = []
		let timeline = ChatTimelineStore(id: "chat", origin: origin, queries: queries, admit: { _ in true },
		                                 readAnchor: { anchor }, saveAnchor: { saved.append($0) },
		                                 markRead: { _, marker in markers.append(marker) })
		timeline.send(.load); await timeline.wait()
		#expect(timeline.state.target == nil)
		#expect(timeline.state.error != nil)
		await timeline.refresh()
		#expect(timeline.state.target == "120")
		timeline.send(.visibleMessages(["120"], foreground: true))
		#expect(timeline.state.target == nil)
		#expect(saved.last?.messageID == "120")
		#expect(markers == [125])
	}

	@Test func staleAdmissionStopsBetweenAdjacentPrimitiveReads() async {
		let queries = queries { $0 <= 60 ? .deleted : .text("visible") }
		var admitted = true; var reads = 0
		queries.historyRead = {
			reads += 1; if reads == 3 {
				admitted = false
			}
		}
		let timeline = ChatTimelineStore(id: "chat", origin: origin, queries: queries, admit: { _ in admitted },
		                                 readAnchor: { anchor })
		timeline.send(.load); await timeline.wait()
		#expect(reads == 3)
		#expect(timeline.state.window.rows.isEmpty)
		#expect(timeline.state.target == nil)
	}

	@Test func cancellationStopsBetweenAdjacentPrimitiveReads() async {
		let queries = queries { $0 <= 60 ? .deleted : .text("visible") }
		var reads = 0
		queries.historyRead = {
			reads += 1
			if reads == 3 {
				withUnsafeCurrentTask { $0?.cancel() }
			}
		}
		let timeline = store(queries)
		timeline.send(.load); await timeline.wait()
		#expect(reads == 3)
		#expect(timeline.state.window.rows.isEmpty)
		#expect(timeline.state.target == nil)
	}

	@Test func interruptedAdjacentReadRetriesOriginalWindow() async {
		let queries = queries { $0 <= 60 ? .deleted : .text("visible") }
		var reads = 0
		queries.historyRead = {
			reads += 1
			if reads == 3 {
				queries.historyHook = { throw ChatError.unavailable }
			}
		}
		let timeline = store(queries)
		timeline.send(.load); await timeline.wait()
		#expect(timeline.state.error == "Message updates stopped.")
		#expect(timeline.state.target == nil)
		queries.historyHook = nil
		await timeline.refresh()
		#expect(timeline.state.target == "61")
		#expect(timeline.state.error == nil)
	}

	@Test func emptyStoredWindowExhaustsWithoutBlockingVisibility() async {
		let queries = ChatQueryFake(); let timeline = store(queries)
		timeline.send(.load); await timeline.wait()
		#expect(timeline.state.target == nil)
		#expect(timeline.state.error == "No surviving messages are available.")
		#expect(queries.beforeRequests.count == 2)
		timeline.send(.visibleMessages([], foreground: true))
		timeline.send(.newest); await timeline.wait()
		#expect(timeline.state.error == nil)
		#expect(timeline.state.atNewest)
	}
}
