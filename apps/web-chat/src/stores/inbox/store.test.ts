import type * as SDK from "@xmtp/browser-sdk";
import type { Conversation, Member } from "@xmtp/browser-sdk";
import { afterEach, describe, expect, it, vi } from "vitest";

import { inboxStore } from "./store";

const sdk = vi.hoisted(() => {
  class Dm {}
  class Group {}
  return { Dm, Group };
});
vi.mock("@xmtp/browser-sdk", async (importOriginal) => ({
  ...(await importOriginal<typeof SDK>()),
  Dm: sdk.Dm,
  Group: sdk.Group,
}));

const own = "a".repeat(64);
const peer = "b".repeat(64);

const member = (inboxId: string, handle?: string): Member => ({
  inboxId,
  identities: [],
  permissionLevel: "member",
  consentState: "unknown",
  handle,
  otherHandles: [],
});

// a DM with the client's inbox and the peer; `members` is read on each call
const dm = (id: string, addedByInboxId: string, members: () => Member[]) =>
  Object.assign(new sdk.Dm(), {
    id,
    addedByInboxId,
    createdAt: { ns: 1n },
    members: vi.fn(async () => members()),
    peerInboxId: vi.fn(async () => peer),
    lastMessage: vi.fn(async () => undefined),
  }) as unknown as Conversation;

const dmName = (id: string) => inboxStore.getState().metadata.get(id)?.name;

afterEach(() => {
  inboxStore.getState().reset();
});

describe("inboxStore", () => {
  it("forgets the own handles of the previous account on reset", () => {
    inboxStore
      .getState()
      .setOwnHandles({ handle: "alice", otherHandles: ["alice#0001"] });
    inboxStore.getState().reset();
    expect(inboxStore.getState().ownHandles).toBeUndefined();
  });

  it("names a DM by its peer's current handle, else the peer's inbox ID", async () => {
    // the peer created the first DM, so the member that is not addedByInboxId
    // is the client itself: the store must find the peer by peerInboxId
    await inboxStore
      .getState()
      .addConversations([
        dm("with-handle", peer, () => [
          member(own, "me"),
          member(peer, "nick"),
        ]),
        dm("without-handle", own, () => [member(own, "me"), member(peer)]),
      ]);
    expect(dmName("with-handle")).toBe("nick");
    expect(dmName("without-handle")).toBe(peer);
  });

  it("renames a DM when its peer's handle changes", async () => {
    let peerHandle = "nick#2341";
    await inboxStore
      .getState()
      .addConversation(
        dm("dm", own, () => [member(own, "me"), member(peer, peerHandle)]),
      );
    expect(dmName("dm")).toBe("nick#2341");
    peerHandle = "nicky";
    await inboxStore.getState().refreshInboxHandles(peer);
    expect(dmName("dm")).toBe("nicky");
    expect(inboxStore.getState().members.get("dm")?.get(peer)?.handle).toBe(
      "nicky",
    );
  });
});
