import Foundation

var controlHistory: [FixtureMessage] = []
struct ControlFixture: Decodable { let messages: [FixtureMessage] }

@main struct StreamControl {
    static func main() throws {
        let fixture = try JSONDecoder().decode(ControlFixture.self, from: Data(contentsOf: URL(fileURLWithPath: CommandLine.arguments[1])))
        controlHistory = fixture.messages
        let ids = fixture.messages.map { "p" + $0.key }
        let expected: Set = ["first", "last"]
        var seen = Set<String>(); var live: [LiveEvent] = []
        try appendLiveEvent("unrelated", expected, &seen, &live) { throw LiveFailure(message: "Unrelated event was decoded") }
        try appendLiveEvent("first", expected, &seen, &live) { LiveEvent(id: "first", kind: "text", text: "value") }
        var duplicateFailed = false
        do { try appendLiveEvent("first", expected, &seen, &live) { LiveEvent(id: "first", kind: "text", text: "value") } }
        catch { duplicateFailed = true }
        try liveRequire(duplicateFailed && live.count == 1, "Duplicate collector control did not fail")
        var missingFailed = false
        do { try requireLiveComplete(seen, expected) } catch { missingFailed = true }
        try liveRequire(missingFailed, "Missing collector control did not fail")
        try appendLiveEvent("last", expected, &seen, &live) { LiveEvent(id: "last", kind: "text", text: "value") }
        try requireLiveComplete(seen, expected)
        print("PASS swift collector duplicate, missing, unrelated, complete")
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
            let faults = ["drop_content", "change_text", "change_reply", "change_attachment", "change_reaction"] + (eager ? ["change_eager_parent", "extra_eager", "duplicate_eager", "future_reaction"] : []) + ["good"]
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
                if fault == "extra_eager" {
                    events[0].eager_reactions = [FixtureReaction(content: "+1", schema: "unicode", action: "added")]
                }
                if fault == "duplicate_eager" {
                    let index = events.firstIndex { !($0.eager_reactions ?? []).isEmpty }!
                    events[index].eager_reactions!.append(events[index].eager_reactions![0])
                }
                if fault == "future_reaction" {
                    let index = events.firstIndex { !($0.eager_reactions ?? []).isEmpty }!
                    events[index].eager_reactions = []
                }
                var failure: String?
                do {
                    let result = try enrichLive(events, ids)
                    try liveRequire(result == controlHistory, "Live semantic result differs from correct history")
                } catch { failure = String(describing: error) }
                let record: [String: Any] = ["target": "swift", "eager": eager, "fault": fault,
                                             "rejected": failure != nil, "failure": failure as Any? ?? NSNull(), "correct_history_messages": controlHistory.count]
                try print(String(data: JSONSerialization.data(withJSONObject: record, options: [.sortedKeys]), encoding: .utf8)!)
                try liveRequire((["good", "future_reaction"].contains(fault)) == (failure == nil), "Live control did not detect the fault")
            }
        }
    }
}
