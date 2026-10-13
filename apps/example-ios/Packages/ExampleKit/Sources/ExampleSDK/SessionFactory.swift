import ExampleCore
import Foundation
import XmtpSdk

@MainActor public final class SDKSessionFactory: ExampleCore.SessionFactory {
	private var clients: [SessionKey: SDKClient] = [:]
	private let allowPrivateAttachments: Bool
	public init(allowPrivateAttachments: Bool = ProcessInfo.processInfo
		.environment["XMTP_ALLOW_PRIVATE_ATTACHMENTS"] == "1")
	{
		self.allowPrivateAttachments = allowPrivateAttachments
	}

	public func client(for key: SessionKey) -> SDKClient? {
		clients[key]
	}

	// Session factory protocol keeps identity, storage and admission inputs explicit.
	// swiftlint:disable:next function_parameter_count
	public func open(
		profile: BackendProfile,
		paths: ProfilePaths,
		wallet: Data,
		databaseKey: Data,
		secrets: any SecretStore,
		key: SessionKey,
		fence: SessionFence,
	) async throws -> any SessionClient {
		let source = try secrets.read(profile.id, ref: "credential") == nil ? nil : SessionCredentialSource(
			secrets: secrets,
			key: key,
			fence: fence,
		)
		let options = try sessionOptions(
			profile: profile,
			paths: paths,
			databaseKey: databaseKey,
			credentials: source,
			allowPrivateAttachments: allowPrivateAttachments,
		)
		let client: SDKClient = if let inbox = profile.inboxID, let identity = profile.identity {
			try await SDKClient.build(
				identity: PublicIdentity(identifier: identity, kind: .ethereum),
				options: options,
				inboxId: inbox,
			)
		} else {
			try await SDKClient.create(signer: localSignerFromPrivateKey(key: wallet), options: options)
		}
		clients[key] = client
		return SDKSessionClient(client: client, onEnd: { [weak self] in self?.clients.removeValue(forKey: key) })
	}
}

func sessionOptions(
	profile: BackendProfile,
	paths: ProfilePaths,
	databaseKey: Data,
	credentials: (any XmtpSdk.CredentialSource)? = nil,
	allowPrivateAttachments: Bool = false,
) throws -> ClientOptions {
	let backend = try BackendURL(profile.url)
	return ClientOptions(
		backend: .options(options: BackendOptions(url: backend.value, credentials: credentials)),
		storage: StorageOptions(
			location: .explicit(dbPath: paths.database.path, attachmentsDir: paths.attachments.path),
			encryptionKey: databaseKey,
		),
		allowOffline: profile.inboxID != nil,
		attachments: AttachmentOptions(allowPrivateNetwork: allowPrivateAttachments && backend.allowPrivateNetwork),
	)
}

@MainActor private final class SessionCredentialSource: XmtpSdk.CredentialSource {
	let secrets: any SecretStore
	let key: SessionKey
	let fence: SessionFence
	@MainActor init(secrets: any SecretStore, key: SessionKey, fence: SessionFence) {
		self.secrets = secrets; self.key = key; self.fence = fence
	}

	func credential() async throws -> Credential {
		try await MainActor.run {
			guard fence.accepts(key), let data = try secrets.read(key.profileID, ref: "credential"), let value = String(
				data: data,
				encoding: .utf8,
			), !value.isEmpty else { throw SessionError.stale }
			return Credential(name: nil, value: value, expiresAtSeconds: Int64.max)
		}
	}
}

@MainActor private final class SDKSessionClient: SessionClient {
	let client: SDKClient
	let onEnd: () -> Void
	init(client: SDKClient, onEnd: @escaping () -> Void) {
		self.client = client; self.onEnd = onEnd
	}

	var inboxID: String {
		client.inboxId()
	}

	var identity: String {
		client.identity().identifier
	}

	func end() async throws {
		defer { onEnd() }; try await client.end()
	}

	func disableNotifications() async throws {
		try await client.disableNotifications()
	}
}
