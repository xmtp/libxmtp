import ExampleCore
import Testing

@Test @MainActor func routeCleanupIsRegisteredBeforeFirstSuspension() async {
	let scope = SessionScope()
	var closes = 0
	let displaced = scope.setCleanup(for: "route", with: { closes += 1 })
	#expect(displaced == nil)
	await scope.runCleanup(for: "route")
	#expect(closes == 1)
	await scope.cancelAndWait()
	#expect(closes == 1)
}

extension SessionCleanupTests {
	@Test @MainActor func keyedRouteCleanupReleasesEveryVisit() async {
		let scope = SessionScope(); let events = CleanupEvents()
		func install(_ name: String) async -> WeakRouteResource {
			let resource = RouteResource(name: name, events: events)
			let weak = WeakRouteResource(resource)
			await scope.replaceCleanup(for: "route") { await resource.close() }
			return weak
		}
		let first = await install("first")
		let second = await install("second")
		#expect(first.value == nil)
		#expect(events.closed == ["first"])
		await scope.runCleanup(for: "route")
		await scope.runCleanup(for: "route")
		#expect(second.value == nil)
		#expect(events.closed == ["first", "second"])
		for name in ["third", "fourth", "fifth"] {
			let visit = await install(name)
			await scope.runCleanup(for: "route")
			#expect(visit.value == nil)
		}
		let pending = await install("pending")
		scope.addCleanup { events.closed.append("session") }
		await scope.cancelAndWait()
		#expect(pending.value == nil)
		#expect(events.closed == ["first", "second", "third", "fourth", "fifth", "session", "pending"])
	}
}

@MainActor private final class CleanupEvents { var closed: [String] = [] }
@MainActor private final class RouteResource {
	let name: String
	let events: CleanupEvents
	init(name: String, events: CleanupEvents) {
		self.name = name; self.events = events
	}

	func close() async {
		events.closed.append(name)
	}
}

@MainActor private final class WeakRouteResource {
	weak var value: RouteResource?
	init(_ value: RouteResource) {
		self.value = value
	}
}
