import { beforeAll, describe, expect, it } from "vitest";

import * as P from "../../../../target/sdk-generated/typescript-wasm/public-values.gen";
import { bridgeError } from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/wire";
import * as B from "../../../../target/sdk-generated/typescript-wasm/xmtp_sdk";

// Public objects keep their binding object private and convert each call.
// These fakes stand in for the binding objects; each records what it received.
class TestProjection extends P.ObjectProjection {
  liftMessage(): never {
    throw new Error("no message in these calls");
  }

  lowerMessage(): never {
    throw new Error("no message in these calls");
  }
}

function binding<T>(fields: Record<string, unknown>): T {
  return fields as T;
}

const identity: P.PublicIdentity = { kind: "ethereum", identifier: "0xabc" };

beforeAll(() => P.installProjection(new TestProjection()));

describe("public objects", () => {
  it("lifts one binding object to one public object", () => {
    const raw = binding<B.GroupLike>({});
    const group = P.wrapGroup(raw);
    expect(group).toBeInstanceOf(P.Group);
    expect(P.wrapGroup(raw)).toBe(group);
    expect(P.unwrapGroup(group)).toBe(raw);
    expect(P.wrapGroup(binding<B.GroupLike>({}))).not.toBe(group);
    expect(() => P.unwrapDm(binding<P.Dm>({}))).toThrow(TypeError);
    expect(Object.keys(group)).not.toContain("raw");
  });

  it("lowers method inputs and lifts results", async () => {
    const sent: unknown[] = [];
    const group = P.wrapGroup(
      binding<B.GroupLike>({
        async members() {
          return [
            {
              inboxId: "inbox",
              identities: [
                { kind: B.PublicIdentityKind.Passkey, identifier: "key" },
              ],
              permissionLevel: B.PermissionLevel.SuperAdmin,
              consentState: B.ConsentState.Allowed,
            },
          ];
        },
        async send(encoded: B.EncodedContent, options?: B.SendOptions) {
          sent.push(encoded, options);
          return "message-id";
        },
      }),
    );
    expect(await group.members()).toEqual([
      {
        inboxId: "inbox",
        identities: [{ kind: "passkey", identifier: "key" }],
        permissionLevel: "superAdmin",
        consentState: "allowed",
      },
    ]);
    const padded = new Uint8Array([9, 1, 2, 9]);
    const encoded: P.EncodedContent = {
      type: {
        authorityId: "example.test",
        typeId: "data",
        versionMajor: 1,
        versionMinor: 0,
      },
      parameters: new Map(),
      content: padded.subarray(1, 3),
    };
    expect(await group.send(encoded, { shouldPush: false })).toBe("message-id");
    const [raw, options] = sent;
    expect([...new Uint8Array((raw as B.EncodedContent).content)]).toEqual([
      1, 2,
    ]);
    // The binding factory fills the defaulted field that the caller left out.
    expect(options).toEqual(B.SendOptions.create({ shouldPush: false }));
  });

  it("projects a conversation as its Group or Dm object", async () => {
    const rawGroup = binding<B.GroupLike>({});
    const rawDm = binding<B.DmLike>({});
    const conversations = P.wrapConversations(
      binding<B.ConversationsLike>({
        async getById(id: string) {
          return id === "group"
            ? B.Conversation.Group.new({ group: rawGroup })
            : undefined;
        },
        async list() {
          return [
            B.Conversation.Group.new({ group: rawGroup }),
            B.Conversation.Dm.new({ dm: rawDm }),
          ];
        },
      }),
    );
    const found = await conversations.getById("group");
    expect(found).toBe(P.wrapGroup(rawGroup));
    expect(await conversations.getById("missing")).toBeUndefined();
    const [group, dm] = await conversations.list();
    expect(group).toBeInstanceOf(P.Group);
    expect(dm).toBeInstanceOf(P.Dm);
    expect(dm).toBe(P.wrapDm(rawDm));
    const lowered = P.lowerConversation(dm!, new TestProjection());
    expect(lowered.tag).toBe(B.Conversation_Tags.Dm);
  });

  it("routes one membership parameter to the inbox or identity method", async () => {
    // The binding has separate inbox and identity methods; the projection
    // chooses one, so it does not depend on a binding union.
    const calls: [string, unknown][] = [];
    const membership = { added: [], removed: [], failedInstallationIds: [] };
    const rawGroup = binding<B.GroupLike>({
      async addMembers(members: unknown) {
        calls.push(["addMembers", members]);
        return membership;
      },
      async addMembersByIdentity(members: unknown) {
        calls.push(["addMembersByIdentity", members]);
        return membership;
      },
    });
    const conversations = P.wrapConversations(
      binding<B.ConversationsLike>({
        async createGroup(members: unknown) {
          calls.push(["createGroup", members]);
          return rawGroup;
        },
        async createGroupWithIdentities(members: unknown) {
          calls.push(["createGroupWithIdentities", members]);
          return rawGroup;
        },
        async createDm(peer: unknown) {
          calls.push(["createDm", peer]);
          return binding<B.DmLike>({});
        },
        async createDmWithIdentity(peer: unknown) {
          calls.push(["createDmWithIdentity", peer]);
          return binding<B.DmLike>({});
        },
      }),
    );
    const lowered = {
      kind: B.PublicIdentityKind.Ethereum,
      identifier: "0xabc",
    };
    await conversations.createGroup([]);
    await conversations.createGroup(["inbox-a", "inbox-b"]);
    const group = await conversations.createGroup([identity]);
    await conversations.createDm("inbox-a");
    await conversations.createDm(identity);
    await group.addMembers(["inbox-a"]);
    await group.addMembers([identity]);
    expect(calls).toEqual([
      ["createGroup", []],
      ["createGroup", ["inbox-a", "inbox-b"]],
      ["createGroupWithIdentities", [lowered]],
      ["createDm", "inbox-a"],
      ["createDmWithIdentity", lowered],
      ["addMembers", ["inbox-a"]],
      ["addMembersByIdentity", [lowered]],
    ]);
    const mixed = ["inbox-a", identity] as unknown as P.PublicIdentity[];
    await expect(conversations.createGroup(mixed)).rejects.toBeInstanceOf(
      P.XmtpError.InvalidArgument,
    );
    expect(calls).toHaveLength(7);
    // The identity methods are not public.
    expect("createGroupWithIdentities" in conversations).toBe(false);
    expect("addMembersByIdentity" in group).toBe(false);
  });

  it("returns null, not undefined, for an unknown DM peer or creator", async () => {
    // The binding reports absence as undefined; the public result is null.
    const dm = P.wrapDm(
      binding<B.DmLike>({
        async peerInboxId() {
          return undefined;
        },
        creatorInboxId() {
          return undefined;
        },
        addedByInboxId() {
          return "adder";
        },
      }),
    );
    expect(await dm.peerInboxId()).toBeNull();
    expect(dm.creatorInboxId).toBeNull();
    expect(dm.addedByInboxId).toBe("adder");
    const group = P.wrapGroup(
      binding<B.GroupLike>({
        creatorInboxId() {
          return undefined;
        },
        addedByInboxId() {
          return undefined;
        },
      }),
    );
    expect(group.creatorInboxId).toBeNull();
    expect(group.addedByInboxId).toBeNull();
    const knownDm = P.wrapDm(
      binding<B.DmLike>({
        creatorInboxId() {
          return "creator";
        },
        addedByInboxId() {
          return undefined;
        },
      }),
    );
    expect(knownDm.creatorInboxId).toBe("creator");
    expect(knownDm.addedByInboxId).toBeNull();
  });

  it("exposes synchronous, argument-free members as readonly getters", () => {
    const group = P.wrapGroup(
      binding<B.GroupLike>({
        id() {
          return "group-id";
        },
        kind() {
          return B.ConversationKind.Group;
        },
      }),
    );
    expect(group.id).toBe("group-id");
    expect(group.kind).toBe("group");
    const getter = Object.getOwnPropertyDescriptor(P.Group.prototype, "id");
    expect(typeof getter?.get).toBe("function");
    expect(getter?.set).toBeUndefined();
  });

  it("leaves defaulted record fields to the binding factory", () => {
    const projection = new TestProjection();
    const location: P.StorageLocation = "inMemory";
    const lowered = P.lowerClientOptions({ storage: { location } }, projection);
    expect(lowered.deviceSync).toBe(true);
    expect(lowered.allowOffline).toBe(false);
    expect(
      P.lowerClientOptions(
        { storage: { location }, deviceSync: false },
        projection,
      ).deviceSync,
    ).toBe(false);
  });

  it("rethrows a binding error as the public error of its code", async () => {
    const details = {
      code: "ClientClosed",
      category: B.ErrorCategory.Lifecycle,
      retryable: false,
      message: "client is closed",
    };
    const plain = new TypeError("not a binding error");
    let thrown: unknown = B.XmtpError.ClientClosed.new(details);
    const group = P.wrapGroup(
      binding<B.GroupLike>({
        async sync() {
          throw thrown;
        },
      }),
    );
    const error: unknown = await group
      .sync()
      .catch((caught: unknown) => caught);
    expect(error).toBeInstanceOf(P.XmtpError.ClientClosed);
    expect(error).toBeInstanceOf(P.XmtpError);
    expect(error).toBeInstanceOf(Error);
    expect(error).not.toBeInstanceOf(P.XmtpError.InvalidArgument);
    expect(error).not.toHaveProperty("tag");
    expect(error).not.toHaveProperty("inner");
    expect((error as InstanceType<typeof P.XmtpError>).details).toEqual({
      code: "ClientClosed",
      category: "lifecycle",
      retryable: false,
      message: "client is closed",
    });
    expect((error as Error).message).toBe("client is closed");
    thrown = plain;
    await expect(group.sync()).rejects.toBe(plain);
    const already = new P.XmtpError.StorageBusy({
      code: "StorageBusy",
      category: "storage",
      retryable: true,
      message: "busy",
    });
    expect(P.publicError(already)).toBe(already);
    // A cancelled call reaches the app as the public Cancelled error.
    const cancelled = P.publicError(bridgeError("cancelled"));
    expect(cancelled).toBeInstanceOf(P.XmtpError.Cancelled);
    expect((cancelled as P.XmtpError).details).toMatchObject({
      code: "Cancelled",
      category: "lifecycle",
      retryable: false,
    });
    expect(P.lowerXmtpError(already, new TestProjection()).tag).toBe(
      B.XmtpError_Tags.StorageBusy,
    );
  });
});
