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
