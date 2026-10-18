import { useEffect, useState } from "react";

import { DetailsStore } from "@/core/details.ts";
import { SdkGroupQueries } from "@/features/details/groupQueries";
import type { ActiveSession } from "@/session";

/**
 * A Details store for one screen (`makeDetailsStore`). Its results apply only
 * while its session is current and the store is not disposed. Its work runs
 * in the session scope, so sign-out awaits it.
 */
export function createDetailsStore(
  session: ActiveSession,
  conversationId: string,
): DetailsStore {
  const generation = session.generation;
  const valid = () => session.isCurrent(generation);
  const store = new DetailsStore(
    new SdkGroupQueries(session, conversationId, valid),
    session.inboxId,
    {
      admit: valid,
      run: (job) => {
        void session.scope.add(() => job());
      },
    },
  );
  return store;
}

interface Shared {
  readonly store: DetailsStore;
  users: number;
}

const shared = new Map<string, Shared>();

function sharedKey(session: ActiveSession, conversationId: string): string {
  return `${session.generation}:${conversationId}`;
}

/**
 * The Details screen's store, shared with its Add member and Edit group
 * sheets (iOS presents both from Details with the Details store). A sheet
 * opened without Details gets its own store.
 */
export function useSharedDetailsStore(
  session: ActiveSession,
  conversationId: string,
): DetailsStore {
  const key = sharedKey(session, conversationId);
  const [entry] = useState(() => {
    const existing = shared.get(key);
    if (existing !== undefined) return existing;
    const created: Shared = {
      store: createDetailsStore(session, conversationId),
      users: 0,
    };
    shared.set(key, created);
    return created;
  });
  useEffect(() => {
    entry.users += 1;
    if (shared.get(key) === undefined) shared.set(key, entry);
    return () => {
      entry.users -= 1;
      // Deferred, so a remount in the same commit keeps the store.
      setTimeout(() => {
        if (entry.users > 0) return;
        entry.store.dispose();
        if (shared.get(key) === entry) shared.delete(key);
      }, 0);
    };
  }, [entry, key]);
  return entry.store;
}

/** A store that only this screen uses; disposed on unmount. */
export function useOwnDetailsStore(
  session: ActiveSession,
  conversationId: string,
): DetailsStore {
  const [store, setStore] = useState(() =>
    createDetailsStore(session, conversationId),
  );
  useEffect(() => {
    // A remount (Fast Refresh) runs this again after the cleanup disposed it.
    if (store.disposed) {
      setStore(createDetailsStore(session, conversationId));
      return;
    }
    store.send({ kind: "refresh" });
    return () => {
      store.dispose();
    };
  }, [store, session, conversationId]);
  return store;
}
