import Foundation
import XCTest
@testable import XMTPiOS
import XMTPTestHelpers

/// Read every field of `serverConfiguration()` and fetch configuration
/// without a client against the shared backend.
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

	// verifies: CONF-061
	func testReadsEveryFieldOfTheSnapshot() async throws {
		let client = try await makeClient()
		defer { try? client.deleteLocalDatabase() }

		let configuration: ServerConfiguration = client.serverConfiguration()

		XCTAssertFalse(configuration.identifier.isEmpty)
		XCTAssertFalse(
			configuration.identifier.contains(where: \.isWhitespace)
		)
		XCTAssertFalse(configuration.serverVersion.isEmpty)
		// An operator that published no minimum sends an empty string.
		let minimum: String = configuration.minLibxmtpVersion
		XCTAssertFalse(minimum.contains(" "))

		let auth: AuthConfiguration = configuration.auth
		let authEnabled: Bool = auth.enabled
		XCTAssertEqual(authEnabled, auth.enabled)
		for key: SigningKeyDescription in auth.keys {
			XCTAssertFalse(key.kid.isEmpty)
			XCTAssertFalse(key.alg.isEmpty)
		}
		// Auth off publishes an empty summary.
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
		// The transport caps both at the fixed 25 MiB transport ceiling.
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

	// verifies: CONF-062
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

	// verifies: CONF-074
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

	// verifies: CONF-064
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

	// verifies: CONF-064
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
