import CryptoKit
import Darwin
import UIKit

func sha256(_ bytes: Data) -> String {
    SHA256.hash(data: bytes).map { String(format: "%02x", $0) }.joined()
}

func appPeakMemory() throws -> UInt64 {
    var info = mach_task_basic_info()
    var count = mach_msg_type_number_t(MemoryLayout<mach_task_basic_info>.size / MemoryLayout<integer_t>.size)
    let status = withUnsafeMutablePointer(to: &info) { pointer in
        pointer.withMemoryRebound(to: integer_t.self, capacity: Int(count)) {
            task_info(mach_task_self_, task_flavor_t(MACH_TASK_BASIC_INFO), $0, &count)
        }
    }
    try require(status == KERN_SUCCESS && info.resident_size_max > 0, "App memory query failed")
    return info.resident_size_max
}

func processOperation(_ envelopeURL: URL) async {
    let responseURL = envelopeURL.deletingLastPathComponent().appendingPathComponent("response.json")
    var response: [String: Any] = [:]
    do {
        guard let envelope = try JSONSerialization.jsonObject(with: Data(contentsOf: envelopeURL)) as? [String: Any],
              let operation = envelope["operation_id"] as? String,
              let requestJSON = envelope["request_json"] as? String,
              let requestDigest = envelope["request_sha256"] as? String,
              let stateKey = envelope["state_key"] as? String
        else {
            throw BenchFailure(message: "Invalid operation envelope")
        }
        response = ["operation_id": operation, "request_sha256": requestDigest]
        let bytes = Data(requestJSON.utf8)
        try require(sha256(bytes) == requestDigest, "Original request digest mismatch")
        guard let request = try JSONSerialization.jsonObject(with: bytes) as? [String: Any],
              let phase = request["phase"] as? String else { throw BenchFailure(message: "Invalid request") }
        try require(request["target"] as? String == "swift", "Request target mismatch")
        try require(["setup", "reset", "measure"].contains(phase), "Unknown request phase")
        try require(stateKey.count == 64 && stateKey.allSatisfy(\.isHexDigit), "Invalid state key")
        let root = URL(fileURLWithPath: NSHomeDirectory()).appendingPathComponent("Library/Application Support/xmtp-benchmark/\(stateKey)")
        let config = try load(HostConfig.self, root.appendingPathComponent("host.json"))
        let fixtureBytes = try Data(contentsOf: root.appendingPathComponent("fixture.json"))
        try require(sha256(fixtureBytes) == request["fixture_sha256"] as? String, "Fixture digest mismatch")
        var result = try await runBenchmark(config, request, root)
        result["peak_memory_bytes"] = try appPeakMemory()
        result["memory_scope"] = "ios-app-resident-high-water"
        response["result"] = result
    } catch {
        response["error"] = ["type": String(describing: type(of: error)), "message": String(describing: error)]
    }
    do {
        try JSONSerialization.data(withJSONObject: response, options: [.sortedKeys]).write(to: responseURL, options: .atomic)
    } catch {
        NSLog("Benchmark response write failed: %@", String(describing: error))
    }
}

@main final class BenchmarkApp: UIResponder, UIApplicationDelegate {
    func application(_: UIApplication, configurationForConnecting session: UISceneSession,
                     options _: UIScene.ConnectionOptions) -> UISceneConfiguration
    {
        let config = UISceneConfiguration(name: "Benchmark", sessionRole: session.role)
        config.delegateClass = BenchmarkScene.self
        return config
    }
}

final class BenchmarkScene: UIResponder, UIWindowSceneDelegate {
    var window: UIWindow?
    func scene(_ scene: UIScene, willConnectTo _: UISceneSession, options _: UIScene.ConnectionOptions) {
        guard let scene = scene as? UIWindowScene else { return }
        let window = UIWindow(windowScene: scene)
        window.rootViewController = UIViewController()
        window.makeKeyAndVisible()
        self.window = window
        let arguments = CommandLine.arguments
        guard let index = arguments.firstIndex(of: "--benchmark-request"), arguments.indices.contains(index + 1) else { return }
        let relative = arguments[index + 1]
        guard !relative.hasPrefix("/"), !relative.split(separator: "/").contains("..") else { return }
        let url = URL(fileURLWithPath: NSHomeDirectory()).appendingPathComponent(relative)
        Task { await processOperation(url) }
    }
}
