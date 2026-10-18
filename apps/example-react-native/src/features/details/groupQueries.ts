// The SDK side of `DetailsStore` (ExampleSDK/GroupQueries.swift and
// IdentityQueries.swift).

import {
  type Conversation,
  type Member,
  Group,
  Timestamp,
  XmtpError,
} from "@xmtp/react-native-sdk";

import type {
  ConversationDetails,
  DetailsMember,
  DetailsMutation,
  GroupQueries,
} from "@/core/details.ts";
import {
  type GroupRole,
  type PermissionWrite,
  policiesFromSet,
  presetLabel,
  sdkPermissionPolicy,
} from "@/core/permissions.ts";
import {
  parseRecipient,
  type Recipient,
  recipientNotFound,
} from "@/core/recipients.ts";
import type { ActiveSession } from "@/session";

/** The conversation is gone or is not the kind an action needs. */
export class ConversationUnavailableError extends Error {
  constructor() {
    super("Conversation is unavailable.");
    this.name = "ConversationUnavailableError";
  }
}

/** The work's session or screen ended; its result is dropped. */
export class StaleWorkError extends Error {
  constructor() {
    super("stale");
    this.name = "StaleWorkError";
  }
}

export function memberRole(member: Member): GroupRole {
  return member.permissionLevel;
}

function detailsMember(member: Member): DetailsMember {
  return {
    id: member.inboxId,
    ...(member.handle === undefined ? {} : { handle: member.handle }),
    otherHandles: member.otherHandles,
    role: memberRole(member),
  };
}

/** Finds the inbox of a parsed recipient (`SDKIdentityQueries.inboxID`). */
async function inboxIdFor(
  session: ActiveSession,
  recipient: Recipient,
): Promise<string | undefined> {
  if (recipient.kind === "inbox") return recipient.inboxId;
  const client = session.client;
  try {
    const inboxId = await client.inboxIdFor(
      recipient.kind === "ethereum"
        ? { identifier: recipient.address, kind: "ethereum" }
        : { identifier: recipient.handle, kind: "handle" },
    );
    if (inboxId === undefined) return undefined;
    if (recipient.kind === "ethereum") {
      // The address must still be an identity of that inbox.
      const states = await client.inboxStates([inboxId], true);
      const associated = states.some(
        (state) =>
          state.inboxId === inboxId &&
          state.identities.some(
            (identity) =>
              identity.kind === "ethereum" &&
              identity.identifier.toLowerCase() === recipient.address,
          ),
      );
      if (!associated) return undefined;
    }
    return inboxId;
  } catch (error) {
    if (error instanceof XmtpError.IdentityNotFound) return undefined;
    throw error;
  }
}

/**
 * Resolves a recipient to a member with its handle
 * (`SDKGroupQueries.resolve`). Throws `RecipientError` for input that does
 * not parse or resolve.
 */
export async function resolveMember(
  session: ActiveSession,
  input: string,
): Promise<DetailsMember> {
  const recipient = parseRecipient(input);
  const id = await inboxIdFor(session, recipient);
  if (id === undefined) throw recipientNotFound(recipient);
  let handle: string | undefined;
  try {
    handle = (await session.client.inboxStates([id], false)).at(0)?.handle;
  } catch {
    handle = undefined;
  }
  return {
    id,
    ...(handle === undefined ? {} : { handle }),
    otherHandles: [],
    role: "member",
  };
}

export class SdkGroupQueries implements GroupQueries {
  readonly #session: ActiveSession;
  readonly #id: string;
  readonly #valid: () => boolean;

  constructor(session: ActiveSession, id: string, valid: () => boolean) {
    this.#session = session;
    this.#id = id;
    this.#valid = valid;
  }

  #check(): void {
    if (!this.#valid()) throw new StaleWorkError();
  }

  async #conversation(): Promise<Conversation> {
    const value = await this.#session.client.conversations.getById(this.#id);
    if (value === undefined) throw new ConversationUnavailableError();
    return value;
  }

  async #group(): Promise<Group> {
    const value = await this.#conversation();
    if (!(value instanceof Group)) throw new ConversationUnavailableError();
    return value;
  }

  async load(): Promise<ConversationDetails> {
    const value = await this.#conversation();
    await value.sync();
    this.#check();
    const members = (await value.members()).map(detailsMember);
    if (value instanceof Group) {
      const state = await value.state();
      return {
        record: { id: this.#id, kind: "group", name: state.name },
        kind: "group",
        description: state.description,
        members,
        policies: policiesFromSet(state.permissions.policySet),
        preset: presetLabel(state.permissions.policyType),
        retentionNs: state.common.disappearingSettings?.retentionNs ?? 0n,
        pendingLeave: state.membershipState === "pendingRemove",
      };
    }
    const state = await value.state();
    const peer = members.find((member) => member.id !== this.#session.inboxId);
    return {
      record: {
        id: this.#id,
        kind: "dm",
        ...(peer === undefined ? {} : { peerInboxId: peer.id }),
        ...(peer?.handle === undefined ? {} : { peerHandle: peer.handle }),
      },
      kind: "dm",
      description: "",
      members,
      policies: [],
      preset: "Custom",
      retentionNs: state.disappearingSettings?.retentionNs ?? 0n,
      pendingLeave: false,
    };
  }

  resolve(input: string): Promise<DetailsMember> {
    return resolveMember(this.#session, input);
  }

  async permission(write: PermissionWrite): Promise<void> {
    this.#check();
    const group = await this.#group();
    this.#check();
    await group.updatePermission(
      write.kind,
      sdkPermissionPolicy(write.policy),
      write.field,
    );
    this.#check();
  }

  async mutate(mutation: DetailsMutation): Promise<void> {
    this.#check();
    const value = await this.#conversation();
    this.#check();
    switch (mutation.kind) {
      case "block":
        await value.updateConsentState("denied");
        return;
      case "disappearing":
        await value.updateDisappearingSettings(
          mutation.retentionNs === undefined
            ? undefined
            : {
                from: new Timestamp(BigInt(Date.now()) * 1_000_000n),
                retentionNs: mutation.retentionNs,
              },
        );
        return;
      default:
        break;
    }
    if (!(value instanceof Group)) throw new ConversationUnavailableError();
    switch (mutation.kind) {
      case "add":
        await value.addMembers([mutation.input]);
        return;
      case "remove":
        await value.removeMembers([mutation.id]);
        return;
      case "admin":
        if (mutation.enabled) await value.addAdmin(mutation.id);
        else await value.removeAdmin(mutation.id);
        return;
      case "edit":
        await value.updateName(mutation.name);
        this.#check();
        await value.updateDescription(mutation.description);
        return;
      case "leave":
        await value.requestRemoval();
        return;
    }
  }
}
