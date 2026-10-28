import type { Client, ConsentState, Conversation } from "@xmtp/browser-sdk";
import { useEffect, useState } from "react";
import { hexToBytes } from "viem";

/**
 * The consent state of a conversation; undefined until it is read. Reads it
 * again on each `consent.changed` or `lagged` event for the conversation.
 */
export const useConsentState = (
  client: Client,
  conversation: Conversation,
): ConsentState | undefined => {
  const [read, setRead] = useState<{ id: string; state: ConsentState }>();

  useEffect(() => {
    let active = true;
    let latest = 0;
    let listenerId: bigint | undefined;
    const stop = (id: bigint) => {
      // the client can be closed already; a stopped listener needs nothing
      void client.stopListener(id).catch(() => undefined);
    };
    const load = async () => {
      // a slow older read must not replace a newer one
      const seq = ++latest;
      const state = await conversation.state();
      if (active && seq === latest) {
        setRead({
          id: conversation.id,
          state:
            "common" in state ? state.common.consentState : state.consentState,
        });
      }
    };
    void client
      .startListener(
        {
          kinds: ["consent.changed"],
          group_ids: [hexToBytes(`0x${conversation.id}`)],
        },
        load,
      )
      .then((id) => {
        if (active) {
          listenerId = id;
        } else {
          stop(id);
        }
      })
      .catch(console.error)
      // read after subscribing, so no consent.changed falls between the two
      .then(() => (active ? load() : undefined))
      .catch(console.error);
    return () => {
      active = false;
      if (listenerId !== undefined) {
        stop(listenerId);
      }
    };
  }, [client, conversation]);

  // the caller is not keyed by conversation: never return another one's state
  return read?.id === conversation.id ? read.state : undefined;
};
