@testable import ExampleCore
import Testing

@Test func unreadSelection() {
	for content in [UnreadContent.text, .markdown, .reply, .remoteAttachment] {
		#expect(UnreadRules.includes(
			content: content,
			sender: "peer",
			own: "own",
			published: true,
			insertedAt: 11,
			marker: 10,
		))
	}
	#expect(!UnreadRules.includes(
		content: .other,
		sender: "peer",
		own: "own",
		published: true,
		insertedAt: 11,
		marker: 10,
	))
	#expect(!UnreadRules.includes(content: .text, sender: "own", own: "own", published: true, insertedAt: 11, marker: 10))
	#expect(!UnreadRules.includes(
		content: .text,
		sender: "peer",
		own: "own",
		published: false,
		insertedAt: 11,
		marker: 10,
	))
	#expect(!UnreadRules.includes(content: .text, sender: "peer", own: "own", published: true, insertedAt: 10, marker: 10))
	for missing in 0 ..< 4 {
		let values = (0 ..< 4).map { $0 != missing }; #expect(!UnreadRules.canMarkRead(
			foreground: values[0],
			currentChat: values[1],
			newestPage: values[2],
			atNewest: values[3],
		))
	}
	#expect(UnreadRules.canMarkRead(foreground: true, currentChat: true, newestPage: true, atNewest: true))
}
