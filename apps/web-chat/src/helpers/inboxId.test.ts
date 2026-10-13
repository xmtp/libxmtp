import type { CredentialSource } from "@xmtp/browser-sdk";
import { beforeEach, expect, it, vi } from "vitest";

import { getInboxIdForAddress, getInboxIdForHandle } from "./inboxId";

const mocks = vi.hoisted(() => {
  class IdentityNotFound extends Error {}
  return {
    connect: vi.fn(),
    canMessage: vi.fn(),
    inboxIdFor: vi.fn(),
    inboxStates: vi.fn(),
    IdentityNotFound,
  };
});

vi.mock("@xmtp/browser-sdk", () => ({
  Backend: { connect: mocks.connect },
  Client: {
    canMessage: mocks.canMessage,
    inboxIdFor: mocks.inboxIdFor,
    inboxStates: mocks.inboxStates,
  },
  XmtpError: { IdentityNotFound: mocks.IdentityNotFound },
}));

const address = `0x${"a".repeat(40)}`;
const credentials: CredentialSource = {
  credential: async () => ({ value: "Bearer test", expiresAtSeconds: 0n }),
};
const options = { url: "https://example.com", credentials };
// the connected backend that both calls of one lookup share
const connected = { connected: true };
const identity = { identifier: address, kind: "ethereum" };

beforeEach(() => {
  vi.clearAllMocks();
  mocks.connect.mockResolvedValue(connected);
});

it("does not select an unregistered address from its deterministic inbox ID", async () => {
  mocks.canMessage.mockResolvedValue(new Map([[`ethereum:${address}`, false]]));
  mocks.inboxIdFor.mockResolvedValue("deterministic-inbox-id");

  await expect(getInboxIdForAddress(address, options)).resolves.toBeNull();
  expect(mocks.canMessage).toHaveBeenCalledWith([identity], connected);
  expect(mocks.inboxIdFor).not.toHaveBeenCalled();
});

it("gets the inbox ID after the backend confirms registration", async () => {
  mocks.canMessage.mockResolvedValue(new Map([[`ethereum:${address}`, true]]));
  mocks.inboxIdFor.mockResolvedValue("registered-inbox-id");

  await expect(getInboxIdForAddress(address, options)).resolves.toBe(
    "registered-inbox-id",
  );
  expect(mocks.connect).toHaveBeenCalledExactlyOnceWith(options);
  expect(mocks.inboxIdFor).toHaveBeenCalledWith(identity, connected);
});

it("resolves a handle through one credentialed connection", async () => {
  mocks.inboxIdFor.mockResolvedValue("inbox-b");
  mocks.inboxStates.mockResolvedValue([{ handle: "nicky" }]);

  // "nick" is an old handle of the inbox: the result names its current one
  await expect(getInboxIdForHandle("nick", options)).resolves.toEqual({
    inboxId: "inbox-b",
    handle: "nicky",
  });
  // a credential source answers one backend client, so both calls share it
  expect(mocks.connect).toHaveBeenCalledExactlyOnceWith(options);
  expect(mocks.inboxIdFor).toHaveBeenCalledWith(
    { identifier: "nick", kind: "handle" },
    connected,
  );
  expect(mocks.inboxStates).toHaveBeenCalledWith(["inbox-b"], connected);
});

it("gives no inbox for a handle that no inbox holds", async () => {
  mocks.inboxIdFor.mockRejectedValue(new mocks.IdentityNotFound());

  await expect(getInboxIdForHandle("nobody", options)).resolves.toBeNull();
  expect(mocks.inboxStates).not.toHaveBeenCalled();
});

it("rethrows a handle lookup failure that is not IdentityNotFound", async () => {
  const failure = new Error("backend unavailable");
  mocks.inboxIdFor.mockRejectedValue(failure);

  // the caller shows "Try again", not "No inbox has this handle"
  await expect(getInboxIdForHandle("nick", options)).rejects.toBe(failure);
  expect(mocks.inboxStates).not.toHaveBeenCalled();
});
