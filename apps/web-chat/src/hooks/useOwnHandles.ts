import type { Client } from "@xmtp/browser-sdk";
import { useCallback, useEffect } from "react";

import { useClient } from "@/contexts/XMTPContext";
import { useActions, useOwnHandlesState } from "@/stores/inbox/hooks";
import { inboxStore } from "@/stores/inbox/store";

/**
 * The client whose handles the inbox store holds: the client of the mounted
 * `useHandleSync`. A read that finishes after the app switched to another
 * client, or disconnected, is dropped, so it cannot replace the handles of
 * the current client.
 */
let syncedClient: Client | undefined;

/** Reads the handles of the client's inbox into the inbox store. */
export const loadOwnHandles = async (
  client: Client,
  refreshFromNetwork: boolean = false,
) => {
  const state = await client.inboxState(refreshFromNetwork);
  if (client !== syncedClient) return;
  inboxStore.getState().setOwnHandles({
    handle: state.handle ?? null,
    otherHandles: state.otherHandles,
  });
};

/** The current and other handles of the client's inbox. */
export const useOwnHandles = () => {
  const client = useClient();
  const ownHandles = useOwnHandlesState();
  const refresh = useCallback(
    (refreshFromNetwork: boolean = false) =>
      loadOwnHandles(client, refreshFromNetwork),
    [client],
  );
  return {
    handle: ownHandles?.handle ?? null,
    otherHandles: ownHandles?.otherHandles ?? [],
    loaded: ownHandles !== undefined,
    refresh,
  };
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
        await refreshInboxHandles(inboxId);
        if (inboxId === client.inboxId) {
          await loadOwnHandles(client);
        }
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
