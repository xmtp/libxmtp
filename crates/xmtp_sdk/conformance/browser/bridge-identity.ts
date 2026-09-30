import { expect, it } from "vitest";

import { Group } from "../../../../target/sdk-generated/typescript-wasm/proxy.gen.js";
import {
  PublicIdentityKind,
  XmtpError,
} from "../../../../target/sdk-generated/typescript-wasm/xmtp_sdk.js";
import { host } from "./bridge-support";

export function registerIdentityTests(): void {
  // Each membership union calls the inbox or identity route in the worker. A
  // mixed list fails with InvalidArgument before any call.
  it("routes membership unions and rejects a mixed list before a call", async () => {
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
    await group.removeMembers([identity]);
    await group.removeMembers(["inbox"]);
    await group.removeMembers([]);
    expect(keys).toEqual([
      "Group.removeMembersByIdentity",
      "Group.removeMembers",
      "Group.removeMembers",
    ]);
    const mixed = ["inbox", identity] as unknown as string[];
    const error: unknown = await group.removeMembers(mixed).catch((e) => e);
    expect(XmtpError.InvalidArgument.instanceOf(error)).toBe(true);
    expect(keys).toHaveLength(3);
  });
}
