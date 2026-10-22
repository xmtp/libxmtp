/** A listener that a store calls after each state change. */
export type StoreListener = () => void;

/**
 * A small observable value. Subclasses change it with `setState`; readers use
 * `getState` and `subscribe` (for example through `useSyncExternalStore`).
 */
export class Store<S> {
  #state: S;
  readonly #listeners = new Set<StoreListener>();

  constructor(initial: S) {
    this.#state = initial;
  }

  /** The current state. The same object until the next change. */
  readonly getState = (): S => this.#state;

  /** Calls `listener` after every change until the returned function runs. */
  readonly subscribe = (listener: StoreListener): (() => void) => {
    this.#listeners.add(listener);
    return () => {
      this.#listeners.delete(listener);
    };
  };

  /**
   * Replaces the state and notifies listeners. A state that is the same
   * object (`Object.is`) notifies nobody.
   */
  protected setState(next: S | ((current: S) => S)): void {
    const value =
      typeof next === "function"
        ? (next as (current: S) => S)(this.#state)
        : next;
    if (Object.is(value, this.#state)) return;
    this.#state = value;
    for (const listener of Array.from(this.#listeners)) listener();
  }
}

/** A store whose owner may set its value directly. */
export class ValueStore<S> extends Store<S> {
  set(next: S | ((current: S) => S)): void {
    this.setState(next);
  }
}
