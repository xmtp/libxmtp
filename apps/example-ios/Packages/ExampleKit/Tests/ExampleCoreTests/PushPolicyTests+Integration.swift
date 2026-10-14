@testable import ExampleCore
import Foundation
import Testing

extension PushPolicyTests {
	@Test func deadlineCompletionRunsExactlyOnce() {
		let lease = PushLease(); var results: [Bool] = []
		let completion = PushCompletion(lease: lease) { results.append($0) }
		#expect(completion.finish(false))
		#expect(!lease.active)
		#expect(!completion.finish(true))
		#expect(results == [false])
	}

	@Test func pushRestoreRequiresExistingSignedInProfile() async {
		for failure in 0 ..< 6 {
			let fixture = PushSessionFixture()
			if failure == 0 {
				fixture.files.index.signedIn = false
			}
			if failure == 1 {
				fixture.files.hasDatabase = false
			}
			if failure == 2 {
				fixture.files.index.reset = ResetJournal(profileID: fixture.profile.id, phase: .stopping)
			}
			if failure == 3 {
				fixture.secrets.values["database-key"] = nil
			}
			if failure == 4 {
				fixture.files.index.profiles[0].identity = nil
			}
			if failure == 5 {
				_ = fixture.session.fence.reserve()
			}
			#expect(await fixture.session.restoreForPush() == nil)
			#expect(fixture.factory.opens == 0)
			#expect(fixture.secrets.writes == 0)
		}
		let fixture = PushSessionFixture()
		#expect(await fixture.session.restoreForPush() != nil)
		#expect(fixture.session.backgroundOnly)
		await fixture.session.restore()
		#expect(!fixture.session.backgroundOnly)
		#expect(fixture.factory.opens == 1)
	}

	@Test func replacedPushRestoreClosesUnpublishedClient() async {
		let fixture = PushSessionFixture(); fixture.factory.hold = true
		let task = Task { await fixture.session.restoreForPush() }
		while fixture.factory.waiter == nil {
			await Task.yield()
		}
		_ = fixture.session.fence.reserve()
		fixture.factory.waiter?.resume(); fixture.factory.waiter = nil
		#expect(await task.value == nil)
		#expect(fixture.factory.client.ended)
	}

	@Test func muteWaitsForSDKAndRejectsChangedDestination() async {
		let fixture = PushFixture(); var destination = true
		fixture.gateway.onRead = { destination = false }
		await fixture.controller.setConversationEnabled(false, id: "chat", admit: { destination })
		#expect(fixture.gateway.notificationWrites == 0)
		#expect(!fixture.preferences.muted)
		fixture.gateway.onRead = nil; destination = true
		fixture.gateway.beforeNotificationWrite = { #expect(!fixture.preferences.muted) }
		await fixture.controller.setConversationEnabled(false, id: "chat", admit: { destination })
		#expect(fixture.gateway.notificationWrites == 1)
		#expect(fixture.preferences.muted)
	}

	@Test func promotionUsesTheForegroundPreferences() async {
		let fixture = PushFixture(); let foreground = PushFakePreferences()
		fixture.controller.replacePreferences(foreground)
		await fixture.controller.setConversationEnabled(false, id: "chat")
		#expect(foreground.muted)
		#expect(!fixture.preferences.muted)
		fixture.controller.setEnabled(false)
		#expect(!foreground.enabled)
		#expect(fixture.preferences.enabled)
	}

	@Test func suspendedBindingCannotInstallIntoReplacementSession() async throws {
		let fence = SessionFence(); let key = try #require(fence.replace(UUID()))
		var waiter: CheckedContinuation<Void, Never>?; var installed = false
		let binding = Task {
			if await fence.afterCurrent(key, operation: { await withCheckedContinuation { waiter = $0 } }) {
				installed = true
			}
		}
		while waiter == nil {
			await Task.yield()
		}
		_ = fence.reserve(); waiter?.resume(); await binding.value
		#expect(!installed)
		let fresh = try #require(fence.replace(UUID()))
		#expect(await fence.afterCurrent(fresh, operation: {}))
	}

	@Test func acceptedMuteCommitsAfterLeavingItsScreen() async {
		let fixture = PushFixture(); var destination = true
		fixture.gateway.beforeNotificationWrite = { destination = false }
		#expect(await fixture.controller.setConversationEnabled(false, id: "chat", admit: { destination }))
		#expect(fixture.preferences.muted)
		#expect(fixture.gateway.notificationWrites == 1)
	}

	@Test func committedMuteRefreshesConversationProjection() async {
		let fixture = PushFixture(); let fence = SessionFence(); _ = fence.replace(fixture.owner.session.profileID)
		let routes = RouteOwner(); routes.replace(.chats)
		let store = ConversationsStore(
			session: fixture.owner.session,
			fence: fence,
			routes: routes,
			queries: PushRowQueries(),
			preferences: fixture.preferences,
			ownInboxID: "own",
		)
		await store.refresh()
		#expect(store.state.rows.first?.muted == false)
		fixture.didCommit = { await store.refresh() }
		await fixture.controller.setConversationEnabled(false, id: "chat")
		#expect(store.state.rows.first?.muted == true)
	}
}
