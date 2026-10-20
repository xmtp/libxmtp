import { XmtpError } from "@xmtp/node-sdk";
import { describe, expect, it } from "vitest";

// Rust tests cover reading, fetching, and refreshing the configuration
// (crates/xmtp_sdk/src/tests/client_build.rs). This case checks the generated
// public error classes.
describe("server configuration", () => {
  // verifies: CONF-064
  it("exposes each configuration failure as a distinct public type", () => {
    const cases = [
      ["ConfigurationUnavailable", XmtpError.ConfigurationUnavailable],
      ["ConfigurationInvalid", XmtpError.ConfigurationInvalid],
      ["BackendMismatch", XmtpError.BackendMismatch],
      ["ClientVersionTooOld", XmtpError.ClientVersionTooOld],
      ["AuthRequired", XmtpError.AuthRequired],
      ["ChainNotAccepted", XmtpError.ChainNotAccepted],
    ] as const;
    for (const [code, Type] of cases) {
      const error = new Type({
        code,
        category: "configuration",
        retryable: false,
        message: "configuration failure",
      });
      expect(error).toBeInstanceOf(XmtpError);
      expect(error.details.code).toBe(code);
      for (const [otherCode, Other] of cases)
        if (otherCode !== code) expect(error).not.toBeInstanceOf(Other);
    }
  });
});
