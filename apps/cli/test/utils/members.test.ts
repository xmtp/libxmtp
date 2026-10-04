import type { InboxState, Member } from "@xmtp/node-sdk";
import { describe, expect, it, vi } from "vitest";

import { memberDetails } from "../../src/utils/members.js";

describe("memberDetails", () => {
  it("matches installation IDs to members when states are out of order", async () => {
    const members: Member[] = [
      {
        inboxId: "first",
        identities: [],
        permissionLevel: "member",
        consentState: "unknown",
      },
      {
        inboxId: "second",
        identities: [],
        permissionLevel: "admin",
        consentState: "allowed",
      },
    ];
    const states: InboxState[] = [
      {
        inboxId: "second",
        identities: [],
        installations: [{ id: "second-installation" }],
        recoveryIdentity: { identifier: "second", kind: "ethereum" },
      },
      {
        inboxId: "first",
        identities: [],
        installations: [{ id: "first-installation" }],
        recoveryIdentity: { identifier: "first", kind: "ethereum" },
      },
    ];
    const client = { inboxStates: vi.fn(async () => states) };

    const output = await memberDetails(client, members);

    expect(client.inboxStates).toHaveBeenCalledWith(["first", "second"], true);
    expect(output.map((member) => member.installationIds)).toEqual([
      ["first-installation"],
      ["second-installation"],
    ]);
  });
});
