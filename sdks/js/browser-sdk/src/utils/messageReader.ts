import type { DecodedMessage, DeliveryCursor } from "@xmtp/wasm-bindings";
import type { MessageReaderSource } from "@/MessageStream";
import type { MessageReaderSelection } from "@/types/actions/messageReader";
import type { ClientWorkerAction } from "@/types/actions";
import type { WorkerBridge } from "@/utils/WorkerBridge";

/** Keep tokens in the worker. Only one token can be in transit per reader. */
export const createMessageReader = async (
  worker: WorkerBridge<ClientWorkerAction>,
  selection: MessageReaderSelection & { from?: DeliveryCursor },
): Promise<MessageReaderSource<DecodedMessage | undefined>> => {
  const readerId = crypto.randomUUID();
  let closed = false;
  await worker.action("messageReader.open", { ...selection, readerId });
  return {
    nextDelivery: async () => {
      if (closed || worker.isClosed) return undefined;
      const item = await worker.action("messageReader.next", { readerId });
      if (item === undefined) return undefined;
      const token = { readerId, tokenId: item.tokenId };
      return {
        message: item.message,
        cursor: item.cursor,
        acknowledgement: {
          checkOwner: async () => {
            const valid = await worker.action("messageReader.check", token);
            return valid && !closed && !worker.isClosed;
          },
          acknowledge: () => worker.action("messageReader.acknowledge", token),
          reject: async () => {
            if (!worker.isClosed)
              await worker.action("messageReader.reject", token);
          },
        },
      };
    },
    close: async () => {
      closed = true;
      if (!worker.isClosed)
        await worker.action("messageReader.close", { readerId });
    },
    updateScope: async (groupIds) => {
      await worker.action("messageReader.updateScope", { readerId, groupIds });
    },
    updateFilter: async (conversationType, consentStates) => {
      await worker.action("messageReader.updateFilter", {
        readerId,
        conversationType,
        consentStates,
      });
    },
    catchUpSnapshot: () =>
      worker.action("messageReader.snapshot", { readerId }),
    catchUpChanged: () => worker.action("messageReader.changed", { readerId }),
  };
};
