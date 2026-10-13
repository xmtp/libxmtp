import { Config } from "@oclif/core";
import type { Client as XmtpClient } from "@xmtp/node-sdk";
import { afterEach, describe, expect, it, vi } from "vitest";

import ClientHandleClaim from "@/commands/client/handle-claim";

import {
  createRegisteredIdentity,
  createTestIdentity,
  parseJsonOutput,
  randomHandleName,
  runCommand,
  runWithIdentity,
} from "../../helpers.js";

// an OSC sequence that sets the terminal title
const ESCAPE_INPUT = "\u001b]0;evil\u0007";
// oxlint-disable-next-line no-control-regex
const CONTROL = /[\u0000-\u001f\u007f-\u009f]/;

/** A handle-claim command with a mocked client. */
async function mockedClaim(
  argv: string[],
  inboxState: ReturnType<typeof vi.fn>,
  claimHandle: ReturnType<typeof vi.fn>,
) {
  const command = new ClientHandleClaim(
    argv,
    await Config.load({ root: process.cwd() }),
  );
  const initClient = vi.spyOn(command, "initClient").mockResolvedValue({
    inboxState,
    claimHandle,
  } as unknown as XmtpClient);
  const confirm = vi.spyOn(command, "confirmAction");
  const log = vi.spyOn(command, "log").mockImplementation(() => undefined);
  return { command, initClient, confirm, log };
}

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
    // stderr can also hold core log lines, so match the CLI's own line
    expect(result.stderr.split("\n")).toContain(
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

    expect(inboxState).toHaveBeenNthCalledWith(1, false);
    expect(claimHandle).toHaveBeenCalledExactlyOnceWith("nick");
    expect(log).toHaveBeenCalledExactlyOnceWith(
      "Your handle is now nick.\nOther handles: nick#0001",
    );
  });

  it("refuses text with control characters before any prompt or claim", async () => {
    const claimHandle = vi.fn(async (_handle: string) => undefined);
    const { command, initClient, confirm } = await mockedClaim(
      [ESCAPE_INPUT],
      vi.fn(),
      claimHandle,
    );

    const error = await command.run().then(
      () => undefined,
      (failure: Error) => failure,
    );

    expect(error?.message).toMatch(/ is not a valid handle\./);
    expect(error?.message).not.toMatch(CONTROL);
    expect(initClient).not.toHaveBeenCalled();
    expect(confirm).not.toHaveBeenCalled();
    expect(claimHandle).not.toHaveBeenCalled();
  });

  it("prints no control character from a refused handle", async () => {
    const identity = createTestIdentity();
    const result = await runCommand(
      [
        "client",
        "handle-claim",
        ESCAPE_INPUT,
        "--wallet-key",
        identity.walletKey,
        "--db-encryption-key",
        identity.dbEncryptionKey,
        "--db-path",
        identity.dbPath,
      ],
      { input: "" },
    );

    expect(result.exitCode).not.toBe(0);
    expect(result.stderr.split("\n")).toContain(
      "\\u001b]0;evil\\u0007 is not a valid handle. Start with a letter, then use only lowercase letters, numbers, _ and -, 27 characters or fewer before #. After #, use exactly 4 digits, like nick#0012.",
    );
    expect(result.stdout).toBe("");
    expect(result.stderr).not.toContain("\u001b");
    expect(result.stderr).not.toContain("\u0007");
  });

  it.each([
    ["U+202E", "nick\u202Eeht", "nick\\u202eeht"],
    ["U+2066", "nick\u2066x\u2069", "nick\\u2066x\\u2069"],
  ])(
    "prints no bidi formatting character from input with %s",
    async (_name, input, shown) => {
      const identity = createTestIdentity();
      const result = await runCommand(
        [
          "client",
          "handle-claim",
          input,
          "--wallet-key",
          identity.walletKey,
          "--db-encryption-key",
          identity.dbEncryptionKey,
          "--db-path",
          identity.dbPath,
        ],
        { input: "" },
      );

      expect(result.exitCode).not.toBe(0);
      expect(result.stderr).toContain(`${shown} is not a valid handle.`);
      expect(result.stderr).not.toMatch(/[\u202a-\u202e\u2066-\u2069]/);
    },
  );

  it("reports the error when a failed claim did not land", async () => {
    const failure = new Error("transport closed");
    const claimHandle = vi.fn(async (_handle: string) => {
      throw failure;
    });
    const inboxState = vi.fn(async () => ({
      inboxId: "inbox",
      handle: undefined,
      otherHandles: [],
    }));
    const { command, log } = await mockedClaim(
      ["nick", "--force"],
      inboxState,
      claimHandle,
    );

    await expect(command.run()).rejects.toBe(failure);
    expect(log).not.toHaveBeenCalled();
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
