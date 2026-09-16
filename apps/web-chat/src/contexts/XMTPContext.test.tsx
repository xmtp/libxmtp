import { act, renderHook } from "@testing-library/react";
import { generatePrivateKey } from "viem/accounts";
import type { Client } from "@xmtp/browser-sdk";
import { describe, expect, it } from "vitest";
import { createEphemeralSigner } from "@/helpers/createSigner";
import { XMTPProvider, useXMTP } from "./XMTPContext";

describe("XMTPProvider", () => {
  it("initializes an ephemeral client against XMTP_BACKEND_URL", async () => {
    const backendUrl = import.meta.env.XMTP_BACKEND_URL;
    if (!backendUrl) {
      throw new Error(
        "XMTP_BACKEND_URL must be defined for the XMTP smoke test",
      );
    }
    const { result } = renderHook(() => useXMTP(), { wrapper: XMTPProvider });
    const initialized: { client?: Client } = {};
    await act(async () => {
      initialized.client = await result.current.initialize({
        backendUrl,
        signer: createEphemeralSigner(generatePrivateKey()),
      });
    });
    expect(initialized.client?.inboxId).toBeTruthy();
    await initialized.client?.close();
  });
});
