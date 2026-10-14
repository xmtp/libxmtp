@testable import ExampleCore
import Foundation
import Testing

struct TranscriptDeletionTests {
	@Test(arguments: [ChatContent.text("old private body"), .reply(quote: "old private quote", body: "old private reply")])
	@MainActor func deletedCachedMessageCannotRenderItsOldBody(content: ChatContent) throws {
		let cache = TranscriptCache()
		let position = HistoryPosition(sentAt: 1, cursor: "deleted")
		var window = HistoryWindow()
		window.rows = [
			ChatMessage(id: "deleted", conversationID: "chat", sender: "peer", sentAt: 1, content: content, position: position,
			            reactions: [ReactionRecord(
			            	sender: "peer",
			            	emoji: "x",
			            	added: true,
			            	sentAt: 1,
			            )]),
			ChatMessage(id: "kept", conversationID: "chat", sender: "peer", sentAt: 2, content: .text("kept")),
		]
		window.first = position
		window.last = position
		window.hasOlder = true
		_ = cache.put("chat", window: window)
		_ = cache.put("other", window: window)
		cache.markDeleted(conversationID: "chat", messageID: "deleted")
		let changed = try #require(cache.get("chat"))
		#expect(changed.rows[0].content == .deleted)
		#expect(changed.rows[0].reactions.isEmpty)
		#expect(changed.rows[0].position == position)
		#expect(changed.rows[1] == window.rows[1])
		#expect(changed.first == window.first && changed.last == window.last && changed.hasOlder)
		#expect(cache.get("other") == window)
	}
}
