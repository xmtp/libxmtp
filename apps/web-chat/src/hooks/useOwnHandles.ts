import type { Client } from "@xmtp/browser-sdk";
import { useCallback, useEffect } from "react";

import { useClient } from "@/contexts/XMTPContext";
import {
  useActions,
  useOwnHandlesFailed,
  useOwnHandlesState,
} from "@/stores/inbox/hooks";
import { inboxStore, type OwnHandles } from "@/stores/inbox/store";

/**
 * The client whose handles the inbox store holds: the client of the mounted
 * `useHandleSync`. A read that finishes after the app switched to another
 * client, or disconnected, is dropped, so it cannot replace the handles of
 * the current client.
 */
let syncedClient: Client | undefined;

/**
 * Reads of the same client can finish out of order. Each read takes the next
 * number, and the store keeps the number of the read whose handles it holds.
 * A read stores its handles only when it is newer than that read, so an
 * older read that finishes late never replaces newer handles. A failed read
 * stores nothing: an older read that succeeds after a newer one failed can
 * still fill the store. A failure marks the store failed only while it holds
 * no handles (see `setOwnHandlesFailed`).
 */
let lastRead = 0;
let storedRead = 0;

/**
 * Reads the handles of the client's inbox into the inbox store. Returns the
 * handles that the store holds after the read, or `undefined` when the
 * client is no longer current.
 */
export const loadOwnHandles = async (
  client: Client,
  refreshFromNetwork: boolean = false,
): Promise<OwnHandles | undefined> => {
  const read = ++lastRead;
  let state;
  try {
    state = await client.inboxState(refreshFromNetwork);
  } catch (error) {
    // a failure for a client that is no longer current changes nothing
    if (client === syncedClient && read > storedRead) {
      inboxStore.getState().setOwnHandlesFailed();
    }
    throw error;
  }
  if (client !== syncedClient) return undefined;
  // a newer read already stored its handles
  if (read < storedRead) return inboxStore.getState().ownHandles;
  storedRead = read;
  const handles = {
    handle: state.handle ?? null,
    otherHandles: state.otherHandles,
  };
  inboxStore.getState().setOwnHandles(handles);
  return handles;
};

/** The current and other handles of the client's inbox. */
export const useOwnHandles = () => {
  const client = useClient();
  const ownHandles = useOwnHandlesState();
  const failed = useOwnHandlesFailed();
  const refresh = useCallback(
    (refreshFromNetwork: boolean = false) =>
      loadOwnHandles(client, refreshFromNetwork),
    [client],
  );
  return {
    handle: ownHandles?.handle ?? null,
    otherHandles: ownHandles?.otherHandles ?? [],
    loaded: ownHandles !== undefined,
    // no read worked yet, and the last one failed: the handles are unknown
    failed: ownHandles === undefined && failed,
    refresh,
  };
};

/** The delays before each retry of a refresh after a `handle.changed` event. */
const HANDLE_REFRESH_RETRY_DELAYS_MS = [1_000, 2_000, 4_000];

/**
 * Runs `refresh`. After a failure it logs the error and tries again after
 * each delay in turn, while `current()` holds. It never rejects.
 */
const refreshWithRetry = async (
  refresh: () => Promise<unknown>,
  current: () => boolean,
) => {
  for (const delay of [...HANDLE_REFRESH_RETRY_DELAYS_MS, undefined]) {
    try {
      await refresh();
      return;
    } catch (error) {
      console.error(error);
    }
    if (delay === undefined || !current()) return;
    await new Promise((resolve) => setTimeout(resolve, delay));
    if (!current()) return;
  }
};

/**
 * Loads the client's own handles, and keeps member handles current: on each
 * `handle.changed` event it reads the members of every conversation that
 * holds that inbox again.
 */
export const useHandleSync = (client: Client | undefined) => {
  const { refreshInboxHandles } = useActions();

  useEffect(() => {
    if (!client) return;
    syncedClient = client;
    void loadOwnHandles(client).catch(console.error);
    return () => {
      if (syncedClient === client) {
        syncedClient = undefined;
      }
    };
  }, [client]);

  useEffect(() => {
    if (!client) return;
    let active = true;
    let listenerId: bigint | undefined;
    const stop = (id: bigint) => {
      // the client can be closed already; a stopped listener needs nothing
      void client.stopListener(id).catch(() => undefined);
    };
    void client
      .startListener({ kinds: ["handle.changed"] }, async (event) => {
        if (event.kind !== "handle.changed") return;
        const { inbox_id: inboxId } = event.handle_changed;
        // stop retrying once the app switches clients or the effect cleans up
        const current = () => active && syncedClient === client;
        // one refresh failing must not skip the other
        const refreshes = [
          refreshWithRetry(() => refreshInboxHandles(inboxId), current),
        ];
        if (inboxId === client.inboxId) {
          refreshes.push(
            refreshWithRetry(() => loadOwnHandles(client), current),
          );
        }
        await Promise.all(refreshes);
      })
      .then((id) => {
        if (active) {
          listenerId = id;
        } else {
          stop(id);
        }
      })
      .catch(console.error);
    return () => {
      active = false;
      if (listenerId !== undefined) {
        stop(listenerId);
      }
    };
  }, [client, refreshInboxHandles]);
};
