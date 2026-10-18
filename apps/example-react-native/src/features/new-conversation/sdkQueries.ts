import type { Member } from "@xmtp/react-native-sdk";

import {
  type IdentityPort,
  NewConversationCopy,
  NewConversationError,
  type NewConversationQueries,
  type Person,
  resolvePerson,
} from "@/core/newConversation.ts";
import { type ActiveSession, isIdentityNotFound } from "@/session";

/** Conversations and members that "People from your chats" reads. */
const PEOPLE_CONVERSATIONS = 20;
const PEOPLE_MEMBERS = 50;
const PEOPLE_LIMIT = 100;

/** `SDKIdentityQueries`: the identity reads for recipient resolution. */
export function identityPort(session: ActiveSession): IdentityPort {
  const client = session.client;
  return {
    inboxIdFor: ({ kind, identifier }) =>
      client.inboxIdFor({ kind, identifier }),
    inboxStates: (ids, refresh) => client.inboxStates([...ids], refresh),
    isNotFound: isIdentityNotFound,
  };
}

/** `SDKNewConversationQueries` for one session. */
export function newConversationQueries(
  session: ActiveSession,
): NewConversationQueries {
  const client = session.client;
  const identity = identityPort(session);
  return {
    async people(): Promise<readonly Person[]> {
      const conversations = await client.conversations.list({
        consentStates: ["allowed"],
        limit: PEOPLE_CONVERSATIONS,
      });
      const people: Person[] = [];
      for (const conversation of conversations.slice(0, PEOPLE_CONVERSATIONS)) {
        const members: Member[] = await conversation.members();
        for (const member of members.slice(0, PEOPLE_MEMBERS)) {
          if (
            member.inboxId === session.inboxId ||
            people.some((person) => person.inboxId === member.inboxId)
          ) {
            continue;
          }
          people.push(
            member.handle === undefined
              ? { inboxId: member.inboxId }
              : { inboxId: member.inboxId, handle: member.handle },
          );
          if (people.length === PEOPLE_LIMIT) return people;
        }
      }
      return people;
    },
    maximumMembers(): Promise<number> {
      const configuration = session.serverConfiguration();
      if (configuration === undefined) {
        return Promise.reject(new Error("No server configuration"));
      }
      const maximum = configuration.mls.maxGroupMembers;
      return Promise.resolve(
        maximum > BigInt(Number.MAX_SAFE_INTEGER)
          ? Number.MAX_SAFE_INTEGER
          : Number(maximum),
      );
    },
    resolve: (input) => resolvePerson(input, identity),
    async create({ inboxIds, group, name, description, adminOnly }) {
      if (group) {
        const created = await client.conversations.createGroup([...inboxIds], {
          permissions: { kind: adminOnly ? "adminOnly" : "allMembers" },
          name,
          description,
        });
        return created.id;
      }
      if (inboxIds.length !== 1) {
        throw new NewConversationError(NewConversationCopy.dmNeedsOne);
      }
      const [peer] = inboxIds;
      return (await client.conversations.createDm(peer)).id;
    },
  };
}
