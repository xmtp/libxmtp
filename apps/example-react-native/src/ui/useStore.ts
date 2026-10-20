import { useSyncExternalStore } from "react";

/** The read side of `Store` in `@/core/store.ts`. */
export interface ReadableStore<S> {
  getState: () => S;
  subscribe: (listener: () => void) => () => void;
}

/** Renders with a store's current state and re-renders on each change. */
export function useStore<S>(store: ReadableStore<S>): S {
  return useSyncExternalStore(store.subscribe, store.getState, store.getState);
}
