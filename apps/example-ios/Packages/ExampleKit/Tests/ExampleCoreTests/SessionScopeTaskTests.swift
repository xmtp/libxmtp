@testable import ExampleCore
import Foundation
import Testing

struct SessionScopeTaskTests {
	@Test @MainActor func completedTasksAreNotRetained() async {
		let scope = SessionScope()
		for _ in 0 ..< 1000 {
			await scope.add {}.value
		}
		#expect(scope.trackedTaskCount == 0)
	}

	@Test @MainActor func teardownCancelsUnstartedTaskAndWaitsBeforeFinalCleanup() async {
		let scope = SessionScope()
		var cancellationObserved = false
		var order: [String] = []
		let task = scope.add {
			cancellationObserved = Task.isCancelled
			order.append("reader-ended")
		}
		scope.addFinalCleanup { order.append("client-ended") }
		await scope.cancelAndWait()
		await task.value
		#expect(cancellationObserved)
		#expect(order == ["reader-ended", "client-ended"])
		#expect(scope.trackedTaskCount == 0)
	}
}
