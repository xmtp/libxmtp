import Foundation

struct BenchFailure: Error { let message: String }
func require(_ value: Bool, _ message: String) throws {
    if !value {
        throw BenchFailure(message: message)
    }
}

func delayNanoseconds(_ milliseconds: UInt64) throws -> UInt64 {
    try require(milliseconds <= UInt64.max / 1_000_000, "Delay exceeds the nanosecond limit")
    return milliseconds * 1_000_000
}

func now() -> Double {
    ProcessInfo.processInfo.systemUptime * 1000
}

func hex(_ data: Data) -> String {
    data.map { String(format: "%02x", $0) }.joined()
}

func unhex(_ value: String) -> Data {
    Data(stride(from: 0, to: value.count, by: 2).map {
        let start = value.index(value.startIndex, offsetBy: $0)
        return UInt8(value[start ..< value.index(start, offsetBy: 2)], radix: 16)!
    })
}

struct Fixture: Codable { let messages: [FixtureMessage]; let callback_delay_ms: UInt64 }
struct Saved: Codable {
    var senderKey: String; var senderAddress: String; var senderPath: String; var senderInbox: String
    var receiverKey: String; var receiverAddress: String; var receiverPath: String; var receiverInbox: String
    var groupId: String; var ids: [String]; var eventIds: [String]
}

final class CallbackClock: @unchecked Sendable {
    private let lock = NSLock(); private var last: Double?; private var calls = 0
    func mark() {
        lock.lock(); defer { lock.unlock() }; last = now(); calls += 1
    }

    func result() -> (Double?, Int) {
        lock.lock(); defer { lock.unlock() }; return (last, calls)
    }
}

struct HostConfig: Decodable { let backend_url: String; let signer_url: String }
func signerHelper(_ config: HostConfig, _ input: [String: String]) async throws -> [String: String] {
    guard let url = URL(string: config.signer_url) else { throw BenchFailure(message: "Invalid signer URL") }
    var request = URLRequest(url: url)
    request.httpMethod = "POST"
    request.setValue("application/json", forHTTPHeaderField: "Content-Type")
    request.httpBody = try JSONSerialization.data(withJSONObject: input)
    let (bytes, response) = try await URLSession.shared.data(for: request)
    try require((response as? HTTPURLResponse)?.statusCode == 200, "Signer request failed")
    return try JSONDecoder().decode([String: String].self, from: bytes)
}

func jsonObject<T: Encodable>(_ value: T) throws -> Any {
    try JSONSerialization.jsonObject(with: JSONEncoder().encode(value))
}

func save<T: Encodable>(_ value: T, _ path: URL) throws {
    try JSONEncoder().encode(value).write(to: path)
}

func load<T: Decodable>(_ type: T.Type, _ path: URL) throws -> T {
    try JSONDecoder().decode(type, from: Data(contentsOf: path))
}

func seed(_ config: HostConfig, _ fixture: Fixture, _ root: URL, _ prefix: String, _ streaming: Bool) async throws -> Saved {
    let sender = try await signerHelper(config, [:]); let receiver = try await signerHelper(config, [:])
    var state = Saved(senderKey: sender["key"]!, senderAddress: sender["address"]!,
                      senderPath: root.appendingPathComponent(prefix + "-sender").path, senderInbox: "",
                      receiverKey: receiver["key"]!, receiverAddress: receiver["address"]!,
                      receiverPath: root.appendingPathComponent(prefix + "-receiver").path, receiverInbox: "",
                      groupId: "", ids: [], eventIds: [])
    let a = try await benchCreate(config, state.senderKey, state.senderAddress, state.senderPath, 0, CallbackClock())
    state.senderInbox = benchInbox(a)
    var b: BenchClient?
    do {
        if streaming {
            b = try await benchCreate(config, state.receiverKey, state.receiverAddress, state.receiverPath, 0, CallbackClock())
            state.receiverInbox = benchInbox(b!)
        }
        let group = try await benchNewGroup(a, streaming ? [state.receiverInbox] : [])
        state.groupId = benchGroupID(group)
        if let b {
            try await benchSync(b); _ = try await benchGroup(b, state.groupId)
        }
        for row in fixture.messages {
            let id = try await benchPrepare(group, row, state.ids, state.senderInbox)
            state.ids.append(id); state.eventIds.append(id)
            for reaction in row.reactions {
                try state.eventIds.append(await benchReact(group, id, state.senderInbox, reaction))
            }
        }
        if !streaming {
            try await benchPublish(group); try await benchGroupSync(group)
        }
        if let b {
            try await benchClose(b)
        }; try await benchClose(a)
        return state
    } catch {
        if let b {
            try? await benchClose(b)
        }; try? await benchClose(a); throw error
    }
}

func runBenchmark(_ config: HostConfig, _ request: [String: Any], _ root: URL) async throws -> [String: Any] {
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    let fixture = try load(Fixture.self, root.appendingPathComponent("fixture.json"))
    let phase = request["phase"] as! String; let workload = request["workload"] as? String ?? ""
    let pair = request["pair"] as? Int ?? 0
    var result: [String: Any] = ["ready": true]
    if phase == "setup" {
        try await save(seed(config, fixture, root, "page", false), root.appendingPathComponent("page.json"))
    } else if phase == "reset" {
        if workload == "stream" {
            try await save(seed(config, fixture, root, "stream-\(pair)", true), root.appendingPathComponent("stream-\(pair).json"))
        }
    } else if workload == "cold_start" || workload.hasPrefix("callback_") {
        let account = try await signerHelper(config, [:]); let clock = CallbackClock()
        let start = now()
        let client = try await benchCreate(config, account["key"]!, account["address"]!,
                                           root.appendingPathComponent("\(workload)-\(pair)").path,
                                           workload == "callback_slow" ? fixture.callback_delay_ms : 0, clock)
        let finished = now(); try await benchClose(client)
        let (entered, count) = clock.result()
        try require(!workload.hasPrefix("callback_") || count > 0, "Signer callback did not run")
        result = ["completed": true, "callback_count": count,
                  "duration_ms": finished - (workload.hasPrefix("callback_") ? entered! : start)]
    } else {
        let state = try load(Saved.self, root.appendingPathComponent(workload == "stream" ? "stream-\(pair).json" : "page.json"))
        let sender = try await benchOpen(config, state.senderAddress, state.senderPath, state.senderInbox)
        let group = try await benchGroup(sender, state.groupId)
        let keys = Dictionary(uniqueKeysWithValues: state.ids.enumerated().map { ($0.element, String($0.offset)) })
        if workload == "page" {
            let start = now(); let page = try await benchPage(group, 1000, keys)
            result = ["duration_ms": now() - start, "observed_messages": page]

        } else if workload == "mobile_lift" {
            guard let lift = try await benchLift(group, pair) else { throw BenchFailure(message: "Missing mobile converter") }
            result = ["mobile_lift": lift, "completed": true]
        } else {
            let receiver = try await benchOpen(config, state.receiverAddress, state.receiverPath, state.receiverInbox)
            let receivedGroup = try await benchGroup(receiver, state.groupId)
            let stream = try await benchStream(receiver, receivedGroup)
            // Same untimed subscription grace on each installed package.
            try await Task.sleep(nanoseconds: 1_000_000_000)
            let start = now(); let publishing = Task { try await benchPublish(group) }
            var seen = Set<String>(); let expected = Set(state.eventIds)
            var live: [LiveEvent] = []
            for try await message in stream {
                try appendLiveEvent(message.id, expected, &seen, &live) { try benchLive(message) }
                if seen.count == expected.count {
                    break
                }
            }
            try await publishing.value
            try requireLiveComplete(seen, expected)
            let page = try enrichLive(live, state.ids).map(jsonRow)
            result = try ["duration_ms": now() - start, "observed_messages": page,
                          "streamed_events": seen.count, "streamed_primary": state.ids.count,
                          "eager_snapshots": live.filter { $0.kind != "reaction" }.map { event -> [String: Any] in
                              try ["id": event.id, "reactions": event.eager_reactions.map { try jsonObject($0) } ?? NSNull(),
                                   "parent_text": event.eager_parent_text as Any? ?? NSNull()]
                          },
                          "eager_snapshot_validation": "PENDING: reaction snapshot completeness has no public boundary"]
            try await benchClose(receiver)
        }
        try await benchClose(sender)
    }
    result["source"] = ["fixture_sha256": request["fixture_sha256"]!, "package_sha256": request["package_sha256"]!]
    return result
}

func jsonRow(_ row: FixtureMessage) throws -> [String: Any] {
    try ["key": row.key, "text": row.text as Any? ?? NSNull(), "reply_to": row.reply_to as Any? ?? NSNull(),
         "parent_text": row.parent_text as Any? ?? NSNull(), "reactions": jsonObject(row.reactions),
         "attachment": row.attachment.map { try jsonObject($0) } ?? NSNull()]
}
