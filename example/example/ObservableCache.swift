import SwiftUI

// A utility class for storing a SwiftUI Observable cache of entities.
//
// This generic type uses a `loader` to know how to fetch the items.
// Then when the cache is asked for an item by id (`cache[id]`) then it will
// return an observable edition of that item. This will then update
// automatically whenever the cached item is anywhere updated in the app.
//
// tl/dr: this lets your View access `cache[id].value` and then magically
//        get updates whenever that cache value is (re)loaded.
@Observable
class ObservableCache<T> {
	private var itemCache = NSCache<NSString, ObservableItem<T>>()
	private let lock = NSLock()
	private let defaultValue: (String) -> T?
	var loader: ((String) async throws -> T)?

	init(defaultValue: T? = nil, loader: ((String) async throws -> T)? = nil) {
		self.loader = loader
		self.defaultValue = { _ in defaultValue }
	}

	init(defaultValue: @escaping (String) -> T? = { _ in nil }, loader: ((String) async throws -> T)? = nil) {
		self.loader = loader
		self.defaultValue = defaultValue
	}

	// This lets callers get the same @Observable item so that any updates are broadcast.
	subscript(identifier: String) -> ObservableItem<T> {
		let result = getOrCreate(identifier: identifier)
		if !result.fromCache {
			_ = doLoad(identifier: identifier, item: result.item)
		}
		return result.item
	}

	// Empty the cache
	func clear() {
		lock.lock()
		itemCache.removeAllObjects()
		lock.unlock()
	}

	// Permit outside insertion of values (e.g. prefill or update the cache)
	// Note: listeners will be broadcast with the update.
	func insert(identifier: String, value: T) {
		let result = getOrCreate(identifier: identifier)
		result.item.value = value
	}

	// Permit outside initiation of a reload of the value for an item.
	func reload(_ identifier: String) -> Task<T?, Error> {
		let result = getOrCreate(identifier: identifier)
		return doLoad(identifier: identifier, item: result.item)
	}

	// This locks to avoid duplicate entries for the specified identifier.
	private func getOrCreate(identifier: String) -> (item: ObservableItem<T>, fromCache: Bool) {
		lock.lock()
		let key = identifier as NSString
		if let cached = itemCache.object(forKey: key) {
			lock.unlock()
			return (cached, true)
		}
		let item = ObservableItem<T>(identifier: identifier, defaultValue: defaultValue(identifier))
		itemCache.setObject(item, forKey: key)
		lock.unlock()
		return (item, false)
	}

	private func doLoad(identifier: String, item: ObservableItem<T>) -> Task<T?, Error> {
		Task {
			do {
				item.value = try await loader?(identifier)
				return item.value
			} catch {
				print("load error (identifier = \"\(identifier)\"): \(error)")
				return nil
			}
		}
	}
}

@Observable
class ObservableItem<T> {
	let identifier: String
	var value: T?

	init(identifier: String, defaultValue: T? = nil) {
		self.identifier = identifier
		value = defaultValue
	}
}
