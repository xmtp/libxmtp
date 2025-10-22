import Foundation
import XCTest

@testable import XMTPiOS

@available(iOS 15, *)
final class TransactionReferenceTests: XCTestCase {
	override func setUp() {
		super.setUp()
		setupLocalEnv()
	}

	func testCanUseTransactionReferenceCodec() async throws {
		Client.register(codec: TransactionReferenceCodec())

		let fixtures = try await fixtures()
		let alixClient = fixtures.alixClient
		let boInboxId = fixtures.boClient.inboxID

		let conversation = try await alixClient?.conversations.newConversation(with: boInboxId)
		let alixConversation = try XCTUnwrap(conversation)

		let txRef = TransactionReference(
			namespace: "eip155",
			networkId: "0x1",
			reference: "0xabc123",
			metadata: .init(
				transactionType: "transfer",
				currency: "ETH",
				amount: 0.05,
				decimals: 18,
				fromAddress: "0xAlice",
				toAddress: "0xBob"
			)
		)

		try await alixConversation.send(
			content: txRef,
			options: .init(contentType: ContentTypeTransactionReference)
		)

		let messages = try await alixConversation.messages()
		XCTAssertEqual(messages.count, 2)

		if messages.count == 2 {
			let content: TransactionReference? = try messages.first?.content()
			XCTAssertEqual("0xabc123", content?.reference)
			XCTAssertEqual("ETH", content?.metadata?.currency)
			XCTAssertEqual("0x1", content?.networkId)
			XCTAssertEqual("transfer", content?.metadata?.transactionType)
		}
	}
}
