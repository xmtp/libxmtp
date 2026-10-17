import ExampleCore
import XmtpSdk

public struct SDKBackendProbe: ExampleCore.BackendProbe {
	public init() {}
	public func probe(_ url: BackendURL) async throws -> BackendProbeResult {
		let config = try await SDKClient.fetchServerConfiguration(backend: .options(options: BackendOptions(url: url.value)))
		return BackendProbeResult(requiresToken: config.auth.enabled)
	}
}
