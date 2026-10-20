// The package root installs the public projection that the lowering reads.
import "@xmtp/node-sdk";
import { describe, expect, it } from "vitest";

import {
  currentProjection,
  lowerEventFilter,
} from "../dist/public-values.gen.js";

describe("Preferences", () => {
  it("defaults the omitted own-message event filter before binding conversion", () => {
    const projection = currentProjection();
    expect(
      lowerEventFilter({ kinds: ["conversation.joined"] }, projection)
        .referencesOwnMessages,
    ).toBe(false);
    for (const selected of [false, true]) {
      expect(
        lowerEventFilter(
          { kinds: ["message.received"], references_own_messages: selected },
          projection,
        ).referencesOwnMessages,
      ).toBe(selected);
    }
  });
});
