import Foundation
import XCTest
@testable import XMTPiOS
import XMTPTestHelpers

private actor CredentialSource {
	private(set) var calls = 0
	let expireFirst: Bool
	let rejectFirst: Bool

	init(expireFirst: Bool = false, rejectFirst: Bool = false) {
		self.expireFirst = expireFirst
		self.rejectFirst = rejectFirst
	}

	func credential() -> Credential {
		calls += 1
		return Credential(
			value: rejectFirst && calls == 1
				? "Bearer invalid-sdk-auth-test-key-000000000000"
				: "Bearer sdk-auth-test-key-00000000000000000000",
			expiresAtSeconds: expireFirst && calls == 1 ? 0 : 4_102_444_800
		)
	}
}

@available(iOS 15, *)
final class BackendAuthTests: XCTestCase {
	private func api(_ source: CredentialSource) -> ClientOptions.Api {
		var options = localApi()
		options.authCallback = { await source.credential() }
		return options
	}

	func testCallbackWorksForClientCreationAndStaticIdentityCalls() async throws {
		let source = CredentialSource()
		let account = try PrivateKey.generate()
		let client = try await Client.createInMemory(
			account: account,
			options: ClientOptions(api: api(source), dbEncryptionKey: Data())
		)
		let canMessage = try await Client.canMessage(
			accountIdentities: [account.identity], api: api(source)
		)
		XCTAssertEqual(canMessage[account.identity.identifier.lowercased()], true)
		let beforeRefresh = await source.calls
		XCTAssertGreaterThan(beforeRefresh, 0)
		_ = try await client.refreshServerConfiguration()
		let afterRefresh = await source.calls
		XCTAssertEqual(afterRefresh, beforeRefresh, "Configuration must not request credentials")
	}

	func testAuthenticatedConnectionsDoNotReuseOtherCallbacksOrThePublicCache() async throws {
		let publicConnection = try await Client.connectToApiBackend(api: localApi())
		let firstSource = CredentialSource()
		let secondSource = CredentialSource()
		let first = try await Client.connectToApiBackend(api: api(firstSource))
		let second = try await Client.connectToApiBackend(api: api(secondSource))
		XCTAssertFalse(first === publicConnection)
		XCTAssertFalse(second === publicConnection)
		XCTAssertFalse(first === second)
		let identity = try PrivateKey.generate().identity.ffiPrivate
		_ = try await getInboxIdForIdentifier(api: first, accountIdentifier: identity)
		_ = try await getInboxIdForIdentifier(api: second, accountIdentifier: identity)
		let firstCalls = await firstSource.calls
		let secondCalls = await secondSource.calls
		XCTAssertEqual(firstCalls, 1)
		XCTAssertEqual(secondCalls, 1)
	}

	func testCredentialIsCachedUntilExpiry() async throws {
		let source = CredentialSource(expireFirst: true)
		let connection = try await Client.connectToApiBackend(api: api(source))
		let identity = try PrivateKey.generate().identity.ffiPrivate
		for _ in 0 ..< 3 {
			_ = try await getInboxIdForIdentifier(api: connection, accountIdentifier: identity)
		}
		let calls = await source.calls
		XCTAssertEqual(calls, 2)
	}

	func testCallbackFailureDoesNotExposeAppErrorText() async throws {
		var options = localApi()
		options.authCallback = {
			throw NSError(domain: "private-token-must-not-escape", code: 1)
		}
		do {
			_ = try await Client.getOrCreateInboxId(
				api: options, publicIdentity: PrivateKey.generate().identity
			)
			XCTFail("A failed credential callback must fail the request")
		} catch {
			XCTAssertFalse(String(describing: error).contains("private-token-must-not-escape"))
		}
	}

	func testRejectedCredentialRequestsAReplacement() async throws {
		let source = CredentialSource(rejectFirst: true)
		let options = api(source)
		let configuration = try await Client.fetchServerConfiguration(url: options.backendUrl)
		_ = try await Client.getOrCreateInboxId(
			api: options, publicIdentity: PrivateKey.generate().identity
		)
		let calls = await source.calls
		XCTAssertEqual(calls, configuration.auth.enabled ? 2 : 1)
	}

	func testCredentialAdapterPreservesHeaderAndExpiryWithoutDiagnosticDisclosure() async throws {
		let credential = Credential(name: "x-auth", value: "private-value", expiresAtSeconds: 123)
		let adapter = FfiAuthCallbackAdapter { credential }
		let ffi = try await adapter.onAuthRequired()
		XCTAssertEqual(ffi.name, "x-auth")
		XCTAssertEqual(ffi.value, "private-value")
		XCTAssertEqual(ffi.expiresAtSeconds, 123)
		XCTAssertFalse(String(describing: credential).contains("private-value"))
		XCTAssertFalse(String(reflecting: credential).contains("private-value"))
	}

	func testAdapterPreservesCancellation() async throws {
		let adapter = FfiAuthCallbackAdapter { throw CancellationError() }
		do {
			_ = try await adapter.onAuthRequired()
			XCTFail("A cancelled callback must not return a credential")
		} catch is CancellationError {
			// The adapter preserves cancellation before the FFI error boundary.
		}
	}
}
