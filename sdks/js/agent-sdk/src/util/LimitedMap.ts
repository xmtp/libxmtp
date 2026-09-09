/** Map that evicts the oldest key when its size limit is reached. */
export class LimitedMap<K, V> {
  #map = new Map<K, V>();
  #limit: number;

  /** Create a map with a fixed maximum number of entries. */
  constructor(limit: number) {
    this.#limit = limit;
  }

  /** Store a value, evicting the oldest entry when full. */
  set(key: K, value: V) {
    if (this.#map.size >= this.#limit) {
      const it = this.#map.keys().next();
      if (!it.done) {
        this.#map.delete(it.value);
      }
    }
    this.#map.set(key, value);
  }

  /** Return a value or `undefined` when the key is absent. */
  get(key: K) {
    return this.#map.get(key);
  }
}
