import { expect, it } from "vitest";

import { Group } from "../../../../target/sdk-generated/typescript-wasm/proxy.gen.js";
import { PublicIdentityKind } from "../../../../target/sdk-generated/typescript-wasm/xmtp_sdk.js";
import { host } from "./bridge-support";

export function registerIdentityTests(): void {
  // Each membership method calls its own route in the worker.
  it("forwards the inbox and identity membership methods by name", async () => {
    // The worker proxy has no membership union: the public layer chooses the
    // inbox or identity method and rejects a mixed list before any call.
    const keys: string[] = [];
    const { engine, session } = host(async (key) => {
      keys.push(key);
      return undefined;
    });
    await session.ready();
    const group = new Group(session, engine.registry.add({}, "Group"));
    const identity = {
      identifier: "0x0000000000000000000000000000000000000001",
      kind: PublicIdentityKind.Ethereum,
    };
    await group.removeMembersByIdentity([identity]);
    await group.removeMembers(["inbox"]);
    await group.removeMembers([]);
    expect(keys).toEqual([
      "Group.removeMembersByIdentity",
      "Group.removeMembers",
      "Group.removeMembers",
    ]);
  });
}
