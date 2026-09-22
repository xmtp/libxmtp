import type {
  ConsentState,
  ConversationType,
  DecodedMessage,
  DeliveryCursor,
  MessageCatchUp,
  MessageHistorySnapshot,
} from "@xmtp/wasm-bindings";

export type MessageReaderSelection = {
  groupIds?: string[];
  conversationType?: ConversationType;
  consentStates?: ConsentState[];
};
export type WorkerMessageDelivery = {
  message: DecodedMessage | undefined;
  cursor: DeliveryCursor;
  tokenId: string;
};

export type MessageReaderAction =
  | {
      action: "messageReader.open";
      id: string;
      data: MessageReaderSelection & {
        readerId: string;
        from?: DeliveryCursor;
      };
      result: undefined;
    }
  | {
      action: "messageReader.next";
      id: string;
      data: { readerId: string };
      result: WorkerMessageDelivery | undefined;
    }
  | {
      action: "messageReader.check";
      id: string;
      data: { readerId: string; tokenId: string };
      result: boolean;
    }
  | {
      action: "messageReader.acknowledge";
      id: string;
      data: { readerId: string; tokenId: string };
      result: undefined;
    }
  | {
      action: "messageReader.reject";
      id: string;
      data: { readerId: string; tokenId: string };
      result: undefined;
    }
  | {
      action: "messageReader.close";
      id: string;
      data: { readerId: string };
      result: undefined;
    }
  | {
      action: "messageReader.updateScope";
      id: string;
      data: { readerId: string; groupIds?: string[] };
      result: undefined;
    }
  | {
      action: "messageReader.updateFilter";
      id: string;
      data: {
        readerId: string;
        conversationType?: ConversationType;
        consentStates?: ConsentState[];
      };
      result: undefined;
    }
  | {
      action: "messageReader.snapshot";
      id: string;
      data: { readerId: string };
      result: MessageCatchUp;
    }
  | {
      action: "messageReader.changed";
      id: string;
      data: { readerId: string };
      result: MessageCatchUp;
    }
  | {
      action: "messageReader.history";
      id: string;
      data: MessageReaderSelection & { limit: number };
      result: MessageHistorySnapshot;
    }
  | {
      action: "messageReader.beginningCursor";
      id: string;
      data: Record<string, never>;
      result: DeliveryCursor;
    };
