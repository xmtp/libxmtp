import { clientOptions, createSigner } from "@test/helpers";
import { Client, XmtpError } from "@xmtp/node-sdk";
import { describe, expect, it, vi } from "vitest";

// Rust tests cover credential sources, refresh, and rejection
// (crates/xmtp_sdk/src/tests/shared_auth.rs and the xmtp_api_backend auth
// middleware tests). This case checks that a callback failure crosses the
// native binding as a public error without the callback's text.
describe("Node backend authentication", () => {
  it("sanitizes callback failures through native bindings", async () => {
    const { signer } = createSigner();
    const authCallback = vi.fn(() =>
      Promise.reject(new Error("private refresh response")),
    );
    let failure: unknown;
    try {
      await Client.create(
        signer,
        clientOptions({
          backend: {
            url: process.env.XMTP_BACKEND_URL!,
            credentials: { credential: authCallback },
          },
        }),
      );
    } catch (error) {
      failure = error;
    }
    expect(failure).toBeInstanceOf(XmtpError.CredentialCallbackFailed);
    expect(failure).toMatchObject({
      details: {
        code: "CredentialCallbackFailed",
        category: "callback",
        retryable: true,
      },
    });
    expect(String(failure)).not.toContain("private refresh response");
    expect(authCallback).toHaveBeenCalled();
  });
});
