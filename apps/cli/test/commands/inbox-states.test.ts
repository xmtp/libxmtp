import { describe, expect, it } from "vitest";

import {
  backendFlags,
  createIdentityWithHandle,
  createRegisteredIdentity,
  parseJsonOutput,
  runCommand,
  runWithIdentity,
} from "../helpers.js";

describe("inbox-states", () => {
  it("fetches inbox states for multiple inbox IDs", async () => {
    const user = await createRegisteredIdentity();
    const other1 = await createRegisteredIdentity();
    const other2 = await createRegisteredIdentity();

    const result = await runWithIdentity(user, [
      "inbox-states",
      other1.inboxId,
      other2.inboxId,
      "--json",
    ]);

    expect(result.exitCode).toBe(0);

    const states = parseJsonOutput<
      Array<{
        inboxId: string;
      }>
    >(result.stdout);

    expect(states.length).toBe(2);
    expect(states.map((s) => s.inboxId)).toContain(other1.inboxId);
    expect(states.map((s) => s.inboxId)).toContain(other2.inboxId);
  });

  it("fails without inbox ID arguments", async () => {
    const user = await createRegisteredIdentity();

    const result = await runWithIdentity(user, ["inbox-states", "--json"]);

    expect(result.exitCode).not.toBe(0);
  });

  it("shows the handles of each inbox", async () => {
    const owner = await createIdentityWithHandle();
    const other = await createRegisteredIdentity();

    const json = await runCommand([
      "inbox-states",
      owner.inboxId,
      other.inboxId,
      ...backendFlags(),
      "--json",
    ]);
    expect(json.exitCode).toBe(0);
    const states = parseJsonOutput<
      Array<{ inboxId: string; handle: string | null; otherHandles: string[] }>
    >(json.stdout);
    expect(
      states.map(({ inboxId, handle, otherHandles }) => ({
        inboxId,
        handle,
        otherHandles,
      })),
    ).toEqual([
      { inboxId: owner.inboxId, handle: owner.handle, otherHandles: [] },
      { inboxId: other.inboxId, handle: null, otherHandles: [] },
    ]);

    const human = await runCommand([
      "inbox-states",
      owner.inboxId,
      ...backendFlags(),
    ]);
    expect(human.exitCode).toBe(0);
    expect(human.stdout).toContain(
      `Inbox ID: ${owner.inboxId}\nHandle: ${owner.handle}\n`,
    );
  });
});
