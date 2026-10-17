import type { Client } from "@xmtp/browser-sdk";
import { useCallback, useEffect } from "react";

import { useClient } from "@/contexts/XMTPContext";
import {
  useActions,
  useOwnHandlesFailed,
  useOwnHandlesState,
} from "@/stores/inbox/hooks";
import { inboxStore } from "@/stores/inbox/store";

/**
 * Reads the handles of the client's inbox from its local state into the inbox
 * store. A failure marks the store failed while it holds no handles (see
 * `setOwnHandlesFailed`).
 */
export const loadOwnHandles = async (client: Client): Promise<void> => {
  try {
    const state = await client.inboxState(false);
    inboxStore.getState().setOwnHandles({
      handle: state.handle ?? null,
      otherHandles: state.otherHandles,
    });
  } catch (error) {
    inboxStore.getState().setOwnHandlesFailed();
    throw error;
  }
};

/** The current and other handles of the client's inbox. */
export const useOwnHandles = () => {
  const client = useClient();
  const ownHandles = useOwnHandlesState();
  const failed = useOwnHandlesFailed();
  const refresh = useCallback(() => loadOwnHandles(client), [client]);
  return {
    handle: ownHandles?.handle ?? null,
    otherHandles: ownHandles?.otherHandles ?? [],
    loaded: ownHandles !== undefined,
    // no read worked yet, and the last one failed: the handles are unknown
    failed,
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
        // one refresh failing must not skip the other
        await Promise.all([
          refreshInboxHandles(inboxId).catch(console.error),
          inboxId === client.inboxId
            ? loadOwnHandles(client).catch(console.error)
            : undefined,
        ]);
      })
      .then((id) => {
        if (active) {
          listenerId = id;
        } else {
          stop(id);
        }
      })
      .catch(console.error)
      // read after subscribing, so no handle.changed falls between the two
      .then(() => (active ? loadOwnHandles(client) : undefined))
      .catch(console.error);
    return () => {
      active = false;
      if (listenerId !== undefined) {
        stop(listenerId);
      }
    };
  }, [client, refreshInboxHandles]);
};
