import type { ClientOptions } from "@xmtp/browser-sdk";
import { expect, test, vi } from "vitest";

import { create, signer } from "./helpers";

const WAIT = { timeout: 30_000, interval: 1000 };

const cases: Array<{
  name: string;
  first: ClientOptions["storage"];
  second: ClientOptions["storage"];
}> = [
  {
    name: "in-memory",
    first: { location: "inMemory" },
    second: { location: "inMemory" },
  },
  {
    name: "persistent",
    first: { location: { directory: "device-sync-first" } },
    second: { location: { directory: "device-sync-second" } },
  },
];

test.each(cases)(
  "consent converges across $name installations",
  async ({ first: firstStorage, second: secondStorage }) => {
    const owner = signer();
    const first = await create(owner, {
      deviceSync: true,
      storage: firstStorage,
    });
    const peer = await create();
    const dm = await first.conversations.createDm(peer.inboxId);
    const second = await create(owner, {
      deviceSync: true,
      storage: secondStorage,
    });
    await vi.waitFor(async () => {
      expect((await second.inboxState(true)).installations).toHaveLength(2);
    }, WAIT);
    await dm.sync();
    const restored = await vi.waitFor(async () => {
      await first.conversations.syncAll(undefined);
      await second.conversations.sync();
      const conversation = await second.conversations.getById(dm.id);
      expect(conversation).toBeDefined();
      return conversation!;
    }, WAIT);
    await vi.waitFor(async () => {
      await dm.updateConsentState("allowed");
      await dm.updateConsentState("denied");
      await first.preferences.sync();
      await second.preferences.sync();
      const state = await restored.state();
      expect(
        "common" in state ? state.common.consentState : state.consentState,
      ).toBe("denied");
    }, WAIT);
    await restored.updateConsentState("allowed");
    const state = await restored.state();
    expect(
      "common" in state ? state.common.consentState : state.consentState,
    ).toBe("allowed");
  },
);
