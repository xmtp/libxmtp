import { type ErrorDetails, XmtpError } from "@xmtp/node-sdk";
import { describe, expect, it } from "vitest";

import {
  asciiLowercase,
  claimFailureMessage,
  handleFields,
} from "../../src/utils/handles.js";
import { canMessageKey, publicIdentity } from "../../src/utils/identity.js";

function details(code: string): ErrorDetails {
  return { code, category: "identity", retryable: false, message: code };
}

describe("claimFailureMessage", () => {
  it("prints a plain message for each handle failure", () => {
    expect(
      claimFailureMessage(
        new XmtpError.HandleTaken(details("HandleTaken")),
        "Nick#2341",
        [],
      ),
    ).toBe(
      "nick#2341 is taken. Pick another handle. Run 'xmtp client handle-available nick' to see free handles.",
    );
    expect(
      claimFailureMessage(
        new XmtpError.HandleNotAllowed(details("HandleNotAllowed")),
        "admin",
        [],
      ),
    ).toBe("admin is not allowed on this server. Choose a different name.");
    expect(
      claimFailureMessage(
        new XmtpError.HandleNotAllowed(details("HandleNotAllowed")),
        "nk",
        ["nick", "nk"],
      ),
    ).toBe(
      "nk is not allowed on this server now. It still finds you, but you cannot switch back to it.",
    );
    expect(
      claimFailureMessage(
        new XmtpError.HandleTooSoon(details("HandleTooSoon")),
        "nick",
        [],
      ),
    ).toBe(
      "You changed your handle less than 7 days ago. You can change your handle once every 7 days.",
    );
    expect(
      claimFailureMessage(
        new XmtpError.HandleLimitReached(details("HandleLimitReached")),
        "nick",
        [],
      ),
    ).toBe(
      "This inbox has used all 64 handle changes. Your current handle stays, and so do your other handles.",
    );
    expect(
      claimFailureMessage(
        new XmtpError.InvalidArgument(details("InvalidArgument")),
        "nick.eth",
        [],
      ),
    ).toMatch(/^nick\.eth is not a valid handle\./);
  });

  it("leaves every other error to the caller", () => {
    expect(
      claimFailureMessage(
        new XmtpError.IdentityNotFound(details("IdentityNotFound")),
        "nick",
        [],
      ),
    ).toBeUndefined();
    expect(
      claimFailureMessage(new Error("network down"), "nick", []),
    ).toBeUndefined();
  });
});

describe("handle input", () => {
  it("lowercases ASCII letters only", () => {
    // U+212A KELVIN SIGN lowercases to an ASCII k in JavaScript; the SDK
    // rejects it, so the CLI must not turn it into one.
    expect(asciiLowercase("NickK#2341")).toBe("nickK#2341");
  });

  it("sends a handle to the SDK as typed and keys it by its canonical text", () => {
    const identity = publicIdentity("NickK", "handle");
    expect(identity).toEqual({ kind: "handle", identifier: "NickK" });
    expect(canMessageKey(identity)).toBe("handle:nickK");
    expect(publicIdentity("0xABC", "ethereum")).toEqual({
      kind: "ethereum",
      identifier: "0xabc",
    });
  });

  it("gives null and no other handles for an inbox without a handle", () => {
    expect(handleFields(undefined)).toEqual({
      handle: null,
      otherHandles: [],
    });
    expect(handleFields({ handle: "nick", otherHandles: ["nk"] })).toEqual({
      handle: "nick",
      otherHandles: ["nk"],
    });
  });
});
