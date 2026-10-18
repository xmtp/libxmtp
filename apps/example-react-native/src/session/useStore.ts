import { useSyncExternalStore } from "react";

import type { Store } from "@/core/store.ts";

/** The current state of `store`; the component re-renders on each change. */
export function useStore<S>(store: Store<S>): S {
  return useSyncExternalStore(store.subscribe, store.getState, store.getState);
}
