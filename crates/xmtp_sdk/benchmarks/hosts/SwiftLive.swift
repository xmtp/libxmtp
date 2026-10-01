import Foundation

struct FixtureAttachment: Codable, Equatable { let filename: String; let mime_type: String; let bytes_hex: String }
struct FixtureReaction: Codable, Equatable { let content: String; let schema: String; let action: String }
struct FixtureMessage: Codable, Equatable {
    let key: String; let text: String?; let reply_to: String?; let parent_text: String?
    let reactions: [FixtureReaction]; let attachment: FixtureAttachment?
}

struct LiveEvent: Codable {
    let id: String; let kind: String
    var text: String? = nil; var reference: String? = nil
    var attachment: FixtureAttachment? = nil; var reaction: FixtureReaction? = nil
    var eager_parent_text: String? = nil; var eager_reactions: [FixtureReaction]? = nil
}

struct LiveFailure: Error { let message: String }
func liveRequire(_ value: Bool, _ message: String) throws {
    if !value {
        throw LiveFailure(message: message)
    }
}

/// The collector rejects duplicate expected IDs before it changes live state.
func appendLiveEvent(_ id: String, _ expected: Set<String>, _ seen: inout Set<String>,
                     _ events: inout [LiveEvent], _ decode: () throws -> LiveEvent) throws
{
    guard expected.contains(id) else { return }
    try liveRequire(!seen.contains(id), "Duplicate expected stream event")
    try events.append(decode())
    seen.insert(id)
}

func requireLiveComplete(_ seen: Set<String>, _ expected: Set<String>) throws {
    try liveRequire(seen == expected, "Stream ended with missing fixture messages")
}

/// Both SDKs produce this rich result from actual delivered values inside timing.
func enrichLive(_ events: [LiveEvent], _ ids: [String]) throws -> [FixtureMessage] {
    var byId: [String: LiveEvent] = [:]
    for event in events {
        try liveRequire(byId[event.id] == nil, "Duplicate live event")
        byId[event.id] = event
    }
    let keys = Dictionary(uniqueKeysWithValues: ids.enumerated().map { ($0.element, String($0.offset)) })
    var reactions = Dictionary(uniqueKeysWithValues: ids.map { ($0, [FixtureReaction]()) })
    for event in events where event.kind == "reaction" {
        guard let reference = event.reference, let reaction = event.reaction, reactions[reference] != nil else {
            throw LiveFailure(message: "Missing live reaction target or content")
        }
        reactions[reference]!.append(reaction)
    }
    return try ids.map { id in
        guard let event = byId[id], ["text", "reply", "attachment"].contains(event.kind) else {
            throw LiveFailure(message: "Missing live primary content")
        }
        var parent: String?; var parentText: String?
        if event.kind == "reply" {
            guard let reference = event.reference, let key = keys[reference],
                  let original = byId[reference], original.kind == "text", let text = original.text
            else {
                throw LiveFailure(message: "Missing delivered reply parent")
            }
            parent = key; parentText = text
            try liveRequire(event.eager_parent_text == nil || event.eager_parent_text == text,
                            "Eager reply parent differs from the delivered parent")
        }
        try liveRequire(event.kind == "attachment" || event.text != nil, "Missing live text or reply body")
        try liveRequire(event.kind != "attachment" || event.attachment != nil, "Missing live attachment")
        let delivered = reactions[id]!
        if let eager = event.eager_reactions {
            var remaining = delivered
            for reaction in eager {
                guard let index = remaining.firstIndex(of: reaction) else {
                    throw LiveFailure(message: "Eager reaction is absent from the delivered reactions")
                }
                remaining.remove(at: index)
            }
        }
        return FixtureMessage(key: keys[id]!, text: event.kind == "attachment" ? nil : event.text,
                              reply_to: parent, parent_text: parentText, reactions: delivered, attachment: event.attachment)
    }
}
