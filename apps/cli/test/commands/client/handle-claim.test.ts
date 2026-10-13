import { Config } from "@oclif/core";
import type { Client as XmtpClient } from "@xmtp/node-sdk";
import { afterEach, describe, expect, it, vi } from "vitest";

import ClientHandleClaim from "@/commands/client/handle-claim";

import {
  createRegisteredIdentity,
  parseJsonOutput,
  randomHandleName,
  runWithIdentity,
} from "../../helpers.js";

interface ClaimOutput {
  inboxId: string;
  handle: string | null;
  otherHandles: string[];
}

async function currentHandle(
  identity: Awaited<ReturnType<typeof createRegisteredIdentity>>,
): Promise<string | null> {
  const result = await runWithIdentity(identity, ["client", "info", "--json"]);
  expect(result.exitCode).toBe(0);
  return parseJsonOutput<{ properties: { handle: string | null } }>(
    result.stdout,
  ).properties.handle;
}

afterEach(() => vi.restoreAllMocks());

describe("client handle-claim", () => {
  it("claims a handle typed with capitals and makes it current", async () => {
    const identity = await createRegisteredIdentity();
    const handle = `${randomHandleName()}#0042`;

    const result = await runWithIdentity(identity, [
      "client",
      "handle-claim",
      handle.toUpperCase(),
      "--force",
      "--json",
    ]);

    expect(result.exitCode).toBe(0);
    expect(parseJsonOutput<ClaimOutput>(result.stdout)).toEqual({
      inboxId: identity.inboxId,
      handle,
      otherHandles: [],
    });
    expect(await currentHandle(identity)).toBe(handle);

    const again = await runWithIdentity(identity, [
      "client",
      "handle-claim",
      randomHandleName(),
      "--force",
    ]);
    expect(again.exitCode).not.toBe(0);
    expect(again.stderr).toContain(
      "You changed your handle less than 7 days ago.",
    );
    expect(await currentHandle(identity)).toBe(handle);
  });

  it("prints the new handle in human output", async () => {
    const identity = await createRegisteredIdentity();
    const handle = randomHandleName();

    const result = await runWithIdentity(identity, [
      "client",
      "handle-claim",
      handle,
      "--force",
    ]);

    expect(result.exitCode).toBe(0);
    expect(result.stdout).toBe(`Your handle is now ${handle}.`);
  });

  it("refuses the current handle and does not send a claim", async () => {
    const identity = await createRegisteredIdentity();
    const handle = `${randomHandleName()}#0042`;
    const claimed = await runWithIdentity(identity, [
      "client",
      "handle-claim",
      handle,
      "--force",
    ]);
    expect(claimed.exitCode).toBe(0);

    const result = await runWithIdentity(identity, [
      "client",
      "handle-claim",
      handle.toUpperCase(),
      "--force",
    ]);

    // A claim call here would reach the backend and fail with its answer:
    // the 7-day wait, or a replayed signature when both claims are signed in
    // the same second.
    expect(result.exitCode).not.toBe(0);
    expect(result.stderr).toBe(
      `${handle} is already your current handle. Nothing was claimed.`,
    );
    expect(await currentHandle(identity)).toBe(handle);
  });

  // A live switch back needs a second claim within 7 days (HandleTooSoon),
  // so this test gives the command a client with an older handle.
  it("claims one of the other handles to switch back to it", async () => {
    const command = new ClientHandleClaim(
      ["nick", "--force"],
      await Config.load({ root: process.cwd() }),
    );
    const claimHandle = vi.fn(async (_handle: string) => undefined);
    const inboxState = vi
      .fn()
      .mockResolvedValueOnce({
        inboxId: "inbox",
        handle: "nick#0001",
        otherHandles: ["nick"],
      })
      .mockResolvedValueOnce({
        inboxId: "inbox",
        handle: "nick",
        otherHandles: ["nick#0001"],
      });
    vi.spyOn(command, "initClient").mockResolvedValue({
      inboxState,
      claimHandle,
    } as unknown as XmtpClient);
    const log = vi.spyOn(command, "log").mockImplementation(() => undefined);

    await command.run();

    expect(claimHandle).toHaveBeenCalledExactlyOnceWith("nick");
    expect(log).toHaveBeenCalledExactlyOnceWith(
      "Your handle is now nick.\nOther handles: nick#0001",
    );
  });

  it("reports a taken handle and never claims another one", async () => {
    const owner = await createRegisteredIdentity();
    const other = await createRegisteredIdentity();
    const name = randomHandleName();
    const owned = await runWithIdentity(owner, [
      "client",
      "handle-claim",
      name,
      "--force",
    ]);
    expect(owned.exitCode).toBe(0);

    const result = await runWithIdentity(other, [
      "client",
      "handle-claim",
      name,
      "--force",
    ]);

    expect(result.exitCode).not.toBe(0);
    expect(result.stderr).toContain(
      `${name} is taken. Pick another handle. Run 'xmtp client handle-available ${name}' to see free handles.`,
    );
    expect(await currentHandle(other)).toBeNull();
  });

  it("reports a handle the backend does not allow", async () => {
    const identity = await createRegisteredIdentity();

    const result = await runWithIdentity(identity, [
      "client",
      "handle-claim",
      "Admin",
      "--force",
    ]);

    expect(result.exitCode).not.toBe(0);
    expect(result.stderr).toContain(
      "admin is not allowed on this server. Choose a different name.",
    );
    expect(await currentHandle(identity)).toBeNull();
  });

  it("rejects text outside the handle grammar", async () => {
    const identity = await createRegisteredIdentity();

    const result = await runWithIdentity(identity, [
      "client",
      "handle-claim",
      "1nick",
      "--force",
    ]);

    expect(result.exitCode).not.toBe(0);
    expect(result.stderr).toContain("1nick is not a valid handle.");
  });

  it("needs --force in a non-interactive terminal", async () => {
    const identity = await createRegisteredIdentity();

    const result = await runWithIdentity(
      identity,
      ["client", "handle-claim", randomHandleName()],
      { input: "" },
    );

    expect(result.exitCode).not.toBe(0);
    expect(result.stderr).toContain("--force");
    expect(await currentHandle(identity)).toBeNull();
  });
});
