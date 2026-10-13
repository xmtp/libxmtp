import { afterEach, describe, expect, it } from "vitest";

import { inboxStore } from "./store";

afterEach(() => {
  inboxStore.getState().reset();
});

describe("inboxStore", () => {
  it("forgets the own handles of the previous account on reset", () => {
    inboxStore
      .getState()
      .setOwnHandles({ handle: "alice", otherHandles: ["alice#0001"] });
    inboxStore.getState().reset();
    expect(inboxStore.getState().ownHandles).toBeUndefined();
  });
});
