import { describe, expect, it } from "vitest";

import { config } from "./main";

describe("wallet connectors", () => {
  it("keeps the complete connector set", () => {
    expect(config.connectors.map((connector) => connector.name)).toEqual(
      expect.arrayContaining([
        "Injected",
        "Coinbase Wallet",
        "MetaMask",
        "WalletConnect",
      ]),
    );
  });
});
