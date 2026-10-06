import Foundation
import XCTest
@testable import XmtpSdk

private struct StreamOwnerCodec: ContentCodec {
	let label: String
	let type = ContentTypeId(authorityId: "tests.xmtp.org", typeId: "stream-owner", versionMajor: 1, versionMinor: 0)
	func encode(_ value: String) throws -> EncodedContent {
		EncodedContent(type: type, content: Data(value.utf8))
	}

	func decode(_ encoded: EncodedContent) throws -> String {
		"\(label):\(String(decoding: encoded.content, as: UTF8.self))"
	}
}

final class StreamOwnerTests: XCTestCase {
	// verifies: CTYPE-009, PROC-034
	func testReceiverUsesItsOwnerAndFailsAfterEnd() async throws {
		try await withClients { scope in
			let first = try await scope.create(signer: generateLocalSigner(), codecs: [StreamOwnerCodec(label: "first")])
			let second = try await scope.create(signer: generateLocalSigner(), codecs: [StreamOwnerCodec(label: "second")])
			for (client, label) in [(first, "first"), (second, "second")] {
				let group = try await client.conversations.createGroup(members: [InboxId]())
				let id = try await group.send(StreamOwnerCodec(label: label), value: "body")
				let cursor = try await client.conversations.beginningDeliveryCursor()
				let found = try await client.conversations.getById(id: group.id())
				let listed = try await client.conversations.list().first { $0.id() == group.id() }
				for conversation in [found, listed] {
					let value = try XCTUnwrap(conversation)
					let iterator = try await value.streamMessages(options: .init(from: cursor)).makeAsyncIterator()
					let item = try await within(seconds: 30) { try await iterator.next() }
					XCTAssertEqual(item??.id, id)
					guard case let .custom(_, _, decoded?, nil)? = item??.content else {
						return XCTFail("The stream lost its owner's codec")
					}
					XCTAssertEqual(decoded as? String, "\(label):body")
				}
			}
			let detached = try await first.conversations.createGroup(members: [InboxId]())
			try await first.end()
			do {
				_ = try await detached.streamMessages()
				XCTFail("An ended owner opened a stream")
			} catch XmtpError.ClientClosed {}
			let live = try await second.conversations.createGroup(members: [InboxId]())
			let id = try await live.sendText(text: "still live")
			let iterator = try await live.streamMessages().makeAsyncIterator()
			let item = try await within(seconds: 30) { try await iterator.next() }
			XCTAssertEqual(item??.id, id)
		}
	}

	func testSequenceKeepsWeakOwnershipBeforeReaderOpen() async throws {
		let reads = HeldReads()
		let raw = FakeClient(noHandle: Client.NoHandle())
		raw.fakeConversations = FakeConversations(FakeConversationReader(reads))
		var client: SDKClient? = makeSDKClient(raw)
		weak var owner = client
		let conversations = try XCTUnwrap(client?.conversations)
		let stream = try await conversations.stream()
		client = nil
		guard owner == nil else {
			return XCTFail("The sequence retained its owner before a reader opened")
		}
		do {
			_ = try await conversations.stream()
			XCTFail("A detached receiver opened a stream")
		} catch XmtpError.ClientClosed {}
		do {
			_ = try await stream.makeAsyncIterator().next()
			XCTFail("A sequence with a released owner produced a value")
		} catch is CancellationError {}
	}

	func testActiveIteratorRetainsOwnerUntilCleanup() async throws {
		let reads = HeldReads()
		let raw = FakeClient(noHandle: Client.NoHandle())
		raw.fakeConversations = FakeConversations(FakeConversationReader(reads))
		var client: SDKClient? = makeSDKClient(raw)
		weak var owner = client
		let closed = CloseSignal()
		let conversations = try XCTUnwrap(client?.conversations)
		let stream = try await conversations.stream(options: .init(onClose: closed.record))
		var iterator: SDKConversationStream.Iterator? = stream.makeAsyncIterator()
		let read = Task { [current = iterator!] in try await current.next() }
		await reads.waitForRead()
		client = nil
		XCTAssertNotNil(owner, "The active reader lost its owner")
		await reads.releaseRead()
		_ = try await read.value
		iterator = nil
		_ = try await closed.wait()
		for _ in 0 ..< 100 {
			if owner == nil {
				break
			}
			await Task.yield()
		}
		XCTAssertNil(owner, "Reader cleanup retained its owner")
	}
}
