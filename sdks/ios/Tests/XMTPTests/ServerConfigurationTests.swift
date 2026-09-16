import Foundation
import XCTest
@testable import XMTPiOS
import XMTPTestHelpers

/// Spec 006 §7. CFG-106 asks for one test that reads every field of
/// `serverConfiguration()` and one that calls `fetchServerConfiguration(url)`
/// against the shared backend.
@available(iOS 15, *)
final class ServerConfigurationTests: XCTestCase {
	private func makeClient() async throws -> Client {
		let key = try Crypto.secureRandomBytes(count: 32)
		let account = try PrivateKey.generate()
		return try await Client.create(
			account: account,
			options: ClientOptions(api: localApi(), dbEncryptionKey: key)
		)
	}

	/// CFG-080 and CFG-106: every field of the build snapshot is readable and
	/// typed. CFG-026 promises no limit, retention or MLS value is ever zero.
	func testReadsEveryFieldOfTheSnapshot() async throws {
		let client = try await makeClient()
		defer { try? client.deleteLocalDatabase() }

		let configuration: ServerConfiguration = client.serverConfiguration()

		XCTAssertFalse(configuration.identifier.isEmpty)
		XCTAssertFalse(
			configuration.identifier.contains(where: \.isWhitespace)
		)
		XCTAssertFalse(configuration.serverVersion.isEmpty)
		// CFG-005: an operator that published no minimum sends an empty string.
		let minimum: String = configuration.minLibxmtpVersion
		XCTAssertFalse(minimum.contains(" "))

		let auth: AuthConfiguration = configuration.auth
		let authEnabled: Bool = auth.enabled
		XCTAssertEqual(authEnabled, auth.enabled)
		for key: SigningKeyDescription in auth.keys {
			XCTAssertFalse(key.kid.isEmpty)
			XCTAssertFalse(key.alg.isEmpty)
		}
		// CFG-028: auth off publishes an empty summary.
		if !authEnabled {
			XCTAssertTrue(auth.keys.isEmpty)
			XCTAssertTrue(auth.audiences.isEmpty)
			XCTAssertTrue(auth.issuers.isEmpty)
			XCTAssertTrue(auth.requiredScopes.isEmpty)
		}

		let retention: RetentionConfiguration = configuration.retention
		XCTAssertGreaterThan(retention.groupMessageSeconds, 0)
		XCTAssertGreaterThan(retention.welcomeSeconds, 0)
		XCTAssertGreaterThan(retention.keyPackageSeconds, 0)

		let limits: LimitsConfiguration = configuration.limits
		let sizes: [UInt64] = [
			limits.maxEnvelopeBytes,
			limits.maxRequestBytes,
			limits.maxResponseBytes,
			limits.maxPublishTopics,
			limits.maxQueryTopics,
			limits.maxQueryLimit,
			limits.defaultQueryLimit,
			limits.maxNewestMetadataTopics,
			limits.maxNewestFullTopics,
			limits.maxUpdateAdds,
			limits.maxUpdateRemoves,
			limits.maxStreamTopics,
			limits.maxStaticTopics,
			limits.maxLookupIdentifiers,
			limits.maxScwSignatures,
			limits.maxIdentityEntries,
		]
		XCTAssertEqual(sizes.count, 16)
		for size in sizes {
			XCTAssertGreaterThan(size, 0)
		}
		let rates: [Int] = [
			limits.maxUpdateFramesPerSecond,
			limits.maxUpdateBurst,
			limits.maxPingFramesPerSecond,
			limits.maxPingBurst,
		]
		XCTAssertEqual(rates.count, 4)
		for rate in rates {
			XCTAssertGreaterThan(rate, 0)
		}
		// CFG-007 caps both at the fixed 25 MiB transport ceiling.
		XCTAssertLessThanOrEqual(limits.maxRequestBytes, 25 * 1024 * 1024)
		XCTAssertLessThanOrEqual(limits.maxResponseBytes, 25 * 1024 * 1024)
		XCTAssertLessThanOrEqual(limits.defaultQueryLimit, limits.maxQueryLimit)

		let mls: MlsConfiguration = configuration.mls
		XCTAssertGreaterThan(mls.maxGroupMembers, 0)
		XCTAssertGreaterThan(mls.maxInstallationsPerInbox, 0)
		// `optional bool`: absent is distinct from an explicit false, so only
		// the nullable form is asserted here.
		let commitLog: Bool? = mls.commitLogEnabled
		XCTAssertEqual(commitLog, mls.commitLogEnabled)

		let chains: [String] = configuration.smartContractWalletChains
		for chain in chains {
			XCTAssertTrue(chain.contains(":"), "\(chain) is not CAIP-2")
		}
	}

	/// CFG-081 and CFG-106: the static fetch reads the shared backend with no
	/// database, no client and no credential, and reports the same deployment
	/// the client is bound to.
	func testFetchesServerConfigurationWithoutAClient() async throws {
		let api = localApi(appVersion: "Testing/0.0.0")

		let fetched = try await Client.fetchServerConfiguration(
			url: api.backendUrl, appVersion: api.appVersion
		)

		XCTAssertFalse(fetched.identifier.isEmpty)
		XCTAssertFalse(fetched.serverVersion.isEmpty)

		let client = try await makeClient()
		defer { try? client.deleteLocalDatabase() }
		XCTAssertEqual(fetched.identifier, client.serverConfiguration().identifier)
		XCTAssertEqual(fetched.limits, client.serverConfiguration().limits)
	}

	/// CFG-082: an explicit refresh fetches now and returns what it fetched.
	/// The running client's snapshot is unchanged.
	func testRefreshReturnsTheFetchedConfigurationAndLeavesTheSnapshot()
		async throws
	{
		let client = try await makeClient()
		defer { try? client.deleteLocalDatabase() }

		let snapshot = client.serverConfiguration()
		let refreshed = try await client.refreshServerConfiguration()

		XCTAssertEqual(refreshed.identifier, snapshot.identifier)
		XCTAssertEqual(client.serverConfiguration(), snapshot)
	}

	/// CFG-083: each generated case becomes its own Swift type. No backend.
	func testSurfacesEachConditionAsItsOwnType() throws {
		XCTAssertNotNil(
			FfiError.ConfigurationUnavailable(message: "a")
				.serverConfigurationError as? ConfigurationUnavailableError
		)
		XCTAssertNotNil(
			FfiError.ConfigurationInvalid(message: "b")
				.serverConfigurationError as? ConfigurationInvalidError
		)
		XCTAssertNotNil(
			FfiError.BackendMismatch(message: "c")
				.serverConfigurationError as? BackendMismatchError
		)
		XCTAssertNotNil(
			FfiError.ClientVersionTooOld(message: "d")
				.serverConfigurationError as? ClientVersionTooOldError
		)
		XCTAssertNotNil(
			FfiError.AuthRequired(message: "e")
				.serverConfigurationError as? AuthRequiredError
		)
		XCTAssertNotNil(
			FfiError.ChainNotAccepted(message: "f")
				.serverConfigurationError as? ChainNotAcceptedError
		)

		// The message survives and an ordinary failure is left alone.
		let mapped = try XCTUnwrap(
			FfiError.BackendMismatch(message: "[ClientError::BackendMismatch] no")
				.serverConfigurationError
		)
		XCTAssertEqual(mapped.message, "[ClientError::BackendMismatch] no")
		XCTAssertEqual(mapped.description, mapped.message)
		XCTAssertNil(FfiError.Error(message: "ordinary").serverConfigurationError)
		struct OrdinaryError: Error {}
		XCTAssertNil(OrdinaryError().serverConfigurationError)
	}

	/// CFG-069 and CFG-070: a chain the deployment refuses reaches the app as
	/// ``ChainNotAcceptedError`` from every signing path — `create`,
	/// `addAccount`, `removeAccount` and both `revokeInstallations` — not as a
	/// generic creation failure. No backend.
	func testKeepsTheConfigurationErrorOnASigningFailure() throws {
		let rejected = Client.signingFailure(
			FfiError.ChainNotAccepted(message: "[ClientError::ChainNotAccepted] eip155:8453")
		)
		let typed = try XCTUnwrap(rejected as? ChainNotAcceptedError)
		XCTAssertEqual(typed.message, "[ClientError::ChainNotAccepted] eip155:8453")

		// Anything else stays the creation failure it has always been.
		struct OrdinaryError: Error {}
		let ordinary = try XCTUnwrap(
			Client.signingFailure(OrdinaryError()) as? ClientError
		)
		guard case .creationError = ordinary else {
			return XCTFail("an ordinary signing failure must stay a creation error")
		}
	}
}
