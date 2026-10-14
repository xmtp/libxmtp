import ExampleCore
import Testing

@Test @MainActor func finalSessionCleanupRunsAfterRoutesAndReaders() async {
	let scope = SessionScope(); let cache = TranscriptCache()
	var routeClosed = false; var readerClosed = false; var readerStarted = false
	var finalObservedClosedOwners = false
	scope.add {
		readerStarted = true
		while !Task.isCancelled {
			await Task.yield()
		}
		readerClosed = true
	}
	for _ in 0 ..< 10000 {
		if readerStarted {
			break
		}; await Task.yield()
	}
	#expect(readerStarted)
	scope.setCleanup(for: "chat") {
		await Task.yield()
		_ = cache.put("chat", window: HistoryWindow())
		routeClosed = true
	}
	scope.addFinalCleanup {
		finalObservedClosedOwners = routeClosed && readerClosed
		cache.clear()
	}
	await scope.cancelAndWait()
	#expect(finalObservedClosedOwners)
	#expect(cache.get("chat") == nil)
}
