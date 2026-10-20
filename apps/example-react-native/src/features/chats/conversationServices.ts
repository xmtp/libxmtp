// SDK reads and writes behind the conversation lists.

import {
  type Conversation,
  Group,
  Timestamp,
  standardContentType,
} from "@xmtp/react-native-sdk";

import {
  type ConversationConsent,
  type ConversationListPreferences,
  type ConversationQueries,
  type ConversationRecord,
  UNREAD_CONTENT_KINDS,
  previewText,
} from "@/core/conversationList.ts";
import type { RequestDecisionsOptions } from "@/core/requestDecisions.ts";
import type { ActiveSession } from "@/session";

const kindOf = (conversation: Conversation): "group" | "dm" =>
  conversation instanceof Group ? "group" : "dm";

/** Consent, group name, DM peer and the peer's handle (`stateDetails`). */
async function stateDetails(
  session: ActiveSession,
  conversation: Conversation,
  record: ConversationRecord,
): Promise<ConversationRecord> {
  if (conversation instanceof Group) {
    const state = await conversation.state();
    return {
      ...record,
      name: state.name,
      consent: state.common.consentState,
    };
  }
  const state = await conversation.state();
  const peer = await conversation.peerInboxId();
  if (peer === null) return { ...record, consent: state.consentState };
  let handle: string | undefined;
  try {
    // A failed handle read keeps the row; the inbox ID names it instead.
    await session.identities.load([peer]);
    handle = session.identities.identity(peer)?.handle;
  } catch {
    handle = record.peerHandle;
  }
  return {
    id: record.id,
    kind: record.kind,
    preview: record.preview,
    unread: record.unread,
    ...(record.lastActivity === undefined
      ? {}
      : { lastActivity: record.lastActivity }),
    consent: state.consentState,
    peerInboxId: peer,
    ...(handle === undefined ? {} : { peerHandle: handle }),
  };
}

export function conversationQueries(
  session: ActiveSession,
): ConversationQueries {
  const { client } = session;
  return {
    async list(consent: ConversationConsent, limit: number) {
      const values = await client.conversations.list({
        consentStates: [consent],
        limit,
        orderBy: "lastActivity",
      });
      return values.map((value): ConversationRecord => ({
        id: value.id,
        kind: kindOf(value),
        consent,
        preview: "",
        unread: 0,
      }));
    },
    async details(record, marker) {
      const conversation = await client.conversations.getById(record.id);
      if (conversation === undefined) return record;
      const result = await stateDetails(session, conversation, record);
      const unread = await conversation.countMessages({
        insertedAfter: new Timestamp(marker(result)),
        deliveryStatus: "published",
        contentTypes: UNREAD_CONTENT_KINDS.map((kind) =>
          standardContentType(kind),
        ),
        excludeSenderInboxIds: [session.inboxId],
      });
      const last = await conversation.lastMessage();
      return {
        ...result,
        unread:
          unread > BigInt(Number.MAX_SAFE_INTEGER)
            ? Number.MAX_SAFE_INTEGER
            : Number(unread),
        ...(last === undefined
          ? {}
          : {
              preview: previewText(last.content, last.fallback),
              lastActivity: last.sentAt.date,
            }),
      };
    },
  };
}

/** The list preferences of the session's profile. */
export function listPreferences(
  session: ActiveSession,
): ConversationListPreferences {
  const { preferences } = session;
  return {
    alias: (id) => preferences.alias(id),
    marker: (key) => preferences.marker(key),
    get dismissedRequests() {
      return preferences.dismissedRequests;
    },
  };
}

/** The consent writes and the local dismissal of `RequestDecisionsStore`. */
export function decisionPorts(
  session: ActiveSession,
): Pick<
  RequestDecisionsOptions,
  "setConsent" | "dismissRequest" | "isCurrent"
> {
  const generation = session.generation;
  return {
    async setConsent(id, consent) {
      const conversation = await session.client.conversations.getById(id);
      if (conversation === undefined) return;
      await conversation.updateConsentState(consent);
    },
    dismissRequest: (id) => {
      session.preferences.dismissRequest(id);
    },
    isCurrent: () => session.isCurrent(generation),
  };
}
