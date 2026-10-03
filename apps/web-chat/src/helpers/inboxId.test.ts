import { beforeEach, expect, it, vi } from "vitest";

import { getInboxIdForAddress } from "./inboxId";

const mocks = vi.hoisted(() => {
  class IdentityNotFound extends Error {}
  return {
    canMessage: vi.fn(),
    inboxIdFor: vi.fn(),
    IdentityNotFound,
  };
});

vi.mock("@xmtp/browser-sdk", () => ({
  Client: {
    canMessage: mocks.canMessage,
    inboxIdFor: mocks.inboxIdFor,
  },
  XmtpError: { IdentityNotFound: mocks.IdentityNotFound },
}));

const address = `0x${"a".repeat(40)}`;
const backend = "https://example.com";
const identity = { identifier: address, kind: "ethereum" };

beforeEach(() => vi.clearAllMocks());

it("does not select an unregistered address from its deterministic inbox ID", async () => {
  mocks.canMessage.mockResolvedValue(new Map([[`ethereum:${address}`, false]]));
  mocks.inboxIdFor.mockResolvedValue("deterministic-inbox-id");

  await expect(getInboxIdForAddress(address, backend)).resolves.toBeNull();
  expect(mocks.canMessage).toHaveBeenCalledWith([identity], { url: backend });
  expect(mocks.inboxIdFor).not.toHaveBeenCalled();
});

it("gets the inbox ID after the backend confirms registration", async () => {
  mocks.canMessage.mockResolvedValue(new Map([[`ethereum:${address}`, true]]));
  mocks.inboxIdFor.mockResolvedValue("registered-inbox-id");

  await expect(getInboxIdForAddress(address, backend)).resolves.toBe(
    "registered-inbox-id",
  );
  expect(mocks.inboxIdFor).toHaveBeenCalledWith(identity, { url: backend });
});
