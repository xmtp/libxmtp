import type { InboxState, Member } from "@xmtp/node-sdk";
import { describe, expect, it, vi } from "vitest";

import { memberDetails } from "../../src/utils/members.js";

describe("memberDetails", () => {
  it("uses cached installation IDs when the network is unavailable", async () => {
    const members: Member[] = [
      {
        inboxId: "first",
        identities: [],
        permissionLevel: "member",
        consentState: "unknown",
        otherHandles: [],
      },
      {
        inboxId: "second",
        identities: [],
        permissionLevel: "admin",
        consentState: "allowed",
        otherHandles: [],
      },
    ];
    const states: InboxState[] = [
      {
        inboxId: "second",
        identities: [],
        installations: [{ id: "second-installation" }],
        recoveryIdentity: { identifier: "second", kind: "ethereum" },
        otherHandles: [],
      },
      {
        inboxId: "first",
        identities: [],
        installations: [{ id: "first-installation" }],
        recoveryIdentity: { identifier: "first", kind: "ethereum" },
        otherHandles: [],
      },
    ];
    const client = {
      inboxStates: vi.fn(async (_ids: string[], refresh: boolean) => {
        if (refresh) throw new Error("identity service unavailable");
        return states;
      }),
    };

    const output = await memberDetails(client, members, true);

    expect(client.inboxStates).toHaveBeenCalledWith(["first", "second"], false);
    expect(output.map((member) => member.installationIds)).toEqual([
      ["first-installation"],
      ["second-installation"],
    ]);
  });

  it("gives the current handle, and the other handles only in full output", async () => {
    const members: Member[] = [
      {
        inboxId: "named",
        identities: [],
        permissionLevel: "member",
        consentState: "unknown",
        handle: "nick#2341",
        otherHandles: ["nickm", "nick"],
      },
      {
        inboxId: "unnamed",
        identities: [],
        permissionLevel: "member",
        consentState: "unknown",
        otherHandles: [],
      },
    ];
    const states: InboxState[] = members.map((member) => ({
      inboxId: member.inboxId,
      identities: [],
      installations: [{ id: `${member.inboxId}-installation` }],
      recoveryIdentity: { identifier: member.inboxId, kind: "ethereum" },
      otherHandles: [],
    }));
    const client = { inboxStates: vi.fn(async () => states) };

    const full = await memberDetails(client, members, true);
    expect(
      full.map(({ inboxId, handle, otherHandles }) => ({
        inboxId,
        handle,
        otherHandles,
      })),
    ).toEqual([
      {
        inboxId: "named",
        handle: "nick#2341",
        otherHandles: ["nickm", "nick"],
      },
      { inboxId: "unnamed", handle: null, otherHandles: [] },
    ]);

    const table = await memberDetails(client, members, false);
    expect(table.map((row) => row.handle)).toEqual(["nick#2341", null]);
    for (const row of table) {
      expect(row).not.toHaveProperty("otherHandles");
    }
  });
});
