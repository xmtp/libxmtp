import Foundation

var controlHistory: [FixtureMessage] = []
struct ControlFixture: Decodable { let messages: [FixtureMessage] }

@main struct StreamControl {
    static func main() throws {
        let fixture = try JSONDecoder().decode(ControlFixture.self, from: Data(contentsOf: URL(fileURLWithPath: CommandLine.arguments[1])))
        controlHistory = fixture.messages
        let ids = fixture.messages.map { "p" + $0.key }
        for eager in [false, true] {
            var source: [LiveEvent] = []
            for row in fixture.messages {
                let id = "p" + row.key
                var event = LiveEvent(id: id, kind: row.attachment != nil ? "attachment" : row.reply_to != nil ? "reply" : "text",
                                      text: row.text, reference: row.reply_to.map { "p" + $0 }, attachment: row.attachment)
                if eager {
                    event.eager_parent_text = row.parent_text; event.eager_reactions = row.reactions
                }
                source.append(event)
                for reaction in row.reactions {
                    source.append(LiveEvent(id: "r" + row.key, kind: "reaction", reference: id, reaction: reaction))
                }
            }
            let faults = ["drop_content", "change_text", "change_reply", "change_attachment", "change_reaction"] + (eager ? ["change_eager_parent"] : []) + ["good"]
            for fault in faults {
                var events = source
                if fault == "drop_content" {
                    events[0].text = nil
                }
                if fault == "change_text" {
                    events[0].text = "corrupt live text"
                }
                if fault == "change_reply" {
                    events[1].text = "corrupt live reply"
                }
                if fault == "change_attachment" {
                    let value = events[2].attachment!
                    events[2].attachment = FixtureAttachment(filename: value.filename, mime_type: value.mime_type, bytes_hex: "00")
                }
                if fault == "change_reaction" {
                    let index = events.firstIndex { $0.kind == "reaction" }!
                    events[index].reaction = FixtureReaction(content: "corrupt live reaction", schema: "unicode", action: "added")
                }
                if fault == "change_eager_parent" {
                    events[1].eager_parent_text = "corrupt eager parent"
                }
                var failure: String?
                do {
                    let result = try enrichLive(events, ids)
                    try liveRequire(result == controlHistory, "Live semantic result differs from correct history")
                } catch { failure = String(describing: error) }
                let record: [String: Any] = ["target": "swift", "eager": eager, "fault": fault,
                                             "rejected": failure != nil, "failure": failure as Any? ?? NSNull(), "correct_history_messages": controlHistory.count]
                try print(String(data: JSONSerialization.data(withJSONObject: record, options: [.sortedKeys]), encoding: .utf8)!)
                try liveRequire((fault == "good") == (failure == nil), "Live control did not detect the fault")
            }
        }
    }
}
