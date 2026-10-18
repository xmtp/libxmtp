import { useFocusEffect } from "expo-router";
import { useCallback, useEffect, useRef, useState } from "react";
import type { ListViewToken } from "react-native";

import {
  type ConversationConsent,
  ConversationListStore,
} from "@/core/conversationList.ts";
import {
  type DecisionOutcome,
  RequestDecisionsStore,
} from "@/core/requestDecisions.ts";
import {
  conversationQueries,
  decisionPorts,
  listPreferences,
} from "@/features/chats/conversationServices";
import type { ActiveSession } from "@/session";

/** Stream updates arrive in bursts; one reload covers a burst. */
const RELOAD_DELAY_MS = 250;

/**
 * The route's conversation list (`ConversationsStore`). It loads on mount,
 * reloads from the local database after stream messages, new conversations
 * and consent, handle or deletion events, and reads the first page again
 * each time the screen is focused (`.task { .load }`: a chat may have changed
 * its unread count). It closes on unmount.
 */
export function useConversationList(
  session: ActiveSession,
  consent: ConversationConsent,
): ConversationListStore {
  const create = () =>
    new ConversationListStore({
      consent,
      ownInboxId: session.inboxId,
      queries: conversationQueries(session),
      preferences: listPreferences(session),
      isCurrent: () => session.isCurrent(session.generation),
    });
  const [store, setStore] = useState(create);

  useEffect(() => {
    // Fast Refresh (and Strict Mode) run the cleanup and then the effect
    // again with the same state: replace the closed store.
    if (store.disposed) {
      setStore(create());
      return;
    }
    let timer: ReturnType<typeof setTimeout> | undefined;
    const schedule = () => {
      if (timer !== undefined) return;
      timer = setTimeout(() => {
        timer = undefined;
        void store.refresh();
      }, RELOAD_DELAY_MS);
    };
    // Listeners only schedule: the next stream read waits for them.
    const stops = [
      session.streams.onMessage(schedule),
      session.streams.onConversation(schedule),
      session.streams.onEvent(schedule),
    ];
    store.load();
    return () => {
      if (timer !== undefined) clearTimeout(timer);
      for (const stop of stops) stop();
      store.dispose();
    };
  }, [store, session]);

  // The mount effect loads first; later focus events load again.
  const focused = useRef(false);
  useFocusEffect(
    useCallback(() => {
      if (focused.current) store.load();
      focused.current = true;
    }, [store]),
  );

  return store;
}

/** The route's accept, delete and block decisions (`RequestsStore`). */
export function useRequestDecisions(
  session: ActiveSession,
  onOutcome: (outcome: DecisionOutcome) => void,
): RequestDecisionsStore {
  const outcome = useRef(onOutcome);
  useEffect(() => {
    outcome.current = onOutcome;
  });
  const create = () =>
    new RequestDecisionsStore({
      ...decisionPorts(session),
      onOutcome: (value) => {
        outcome.current(value);
      },
    });
  const [store, setStore] = useState(create);
  useEffect(() => {
    if (store.disposed) {
      setStore(create());
      return;
    }
    return () => {
      store.dispose();
    };
  }, [store]);
  return store;
}

/**
 * A stable `onViewableItemsChanged` that reports the visible rows to the
 * store (SwiftUI `onAppear` per row): details and paging follow them. Items
 * are matched by key (the row ID), so other list items are ignored.
 */
export function useVisibleRowsReporter(
  store: ConversationListStore,
): (info: { viewableItems: ListViewToken[] }) => void {
  const [report] = useState(
    () =>
      ({ viewableItems }: { viewableItems: ListViewToken[] }) => {
        const keys = new Set(viewableItems.map((token) => token.key));
        let first = -1;
        let last = -1;
        store.getState().visible.forEach((row, index) => {
          if (!keys.has(row.id)) return;
          if (first < 0) first = index;
          last = index;
        });
        if (first < 0) return;
        store.visible(first, last - first + 1);
      },
  );
  return report;
}
