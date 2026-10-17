import {
  type ErrorDetails,
  type PublicIdentity,
  XmtpError,
} from "@xmtp/node-sdk";
import { describe, expect, it } from "vitest";

import {
  asciiLowercase,
  claimFailureMessage,
  handleFields,
  invalidHandleMessage,
  isValidHandle,
} from "../../src/utils/handles.js";
import {
  canMessageKey,
  publicIdentity,
  reportUnresolvedHandles,
} from "../../src/utils/identity.js";

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

describe("reportUnresolvedHandles", () => {
  const notFound = () =>
    new XmtpError.IdentityNotFound(details("IdentityNotFound"));
  // a client whose lookups find an inbox only for the handles in `held`
  const client = (held: string[], lookups: string[] = []) => ({
    inboxIdFor: async ({ identifier }: PublicIdentity) => {
      lookups.push(identifier);
      return held.includes(identifier) ? "inbox" : undefined;
    },
  });

  it("names each handle that no inbox holds, once, in canonical text", async () => {
    const lookups: string[] = [];
    const identities = [
      publicIdentity("Gone#0001", "handle"),
      publicIdentity("nick", "handle"),
      publicIdentity("gone#0001", "handle"),
      publicIdentity("0xABC", "ethereum"),
    ];

    await expect(
      reportUnresolvedHandles(client(["nick"], lookups), identities, () =>
        Promise.reject(notFound()),
      ),
    ).rejects.toThrow(/^No inbox has the handle gone#0001\.$/);
    expect(lookups.sort()).toEqual(["gone#0001", "nick"]);
  });

  it("passes the error through when every handle has an inbox", async () => {
    const error = notFound();
    await expect(
      reportUnresolvedHandles(
        client(["nick"]),
        [publicIdentity("nick", "handle")],
        () => Promise.reject(error),
      ),
    ).rejects.toBe(error);
  });

  it("passes the error through when a lookup fails", async () => {
    const error = notFound();
    const failing = {
      inboxIdFor: () => Promise.reject(new Error("network down")),
    };
    await expect(
      reportUnresolvedHandles(failing, [publicIdentity("nick", "handle")], () =>
        Promise.reject(error),
      ),
    ).rejects.toBe(error);
  });

  it("looks nothing up for any other error", async () => {
    const lookups: string[] = [];
    const error = new XmtpError.InvalidArgument(details("InvalidArgument"));
    await expect(
      reportUnresolvedHandles(
        client([], lookups),
        [publicIdentity("nick", "handle")],
        () => Promise.reject(error),
      ),
    ).rejects.toBe(error);
    expect(lookups).toEqual([]);
  });
});

// an OSC sequence that sets the terminal title
const ESCAPE_INPUT = "\u001b]0;evil\u0007";
// oxlint-disable-next-line no-control-regex
const CONTROL = /[\u0000-\u001f\u007f-\u009f]/;
const BIDI = /[\u061c\u200b-\u200f\u202a-\u202e\u2060\u2066-\u2069\ufeff]/;

describe("handle input", () => {
  // without the m flag, $ matches only at the end of the input
  it.each(["nick\n", "nick\r\n", "nick#0001\n"])(
    "refuses a trailing line terminator: %j",
    (input) => {
      expect(isValidHandle(input)).toBe(false);
    },
  );

  it("refuses a handle outside the grammar before the SDK sees it", () => {
    for (const input of [ESCAPE_INPUT, "Nick\u212A", "nick.eth"]) {
      expect(() => publicIdentity(input, "handle")).toThrow(
        "is not a valid handle.",
      );
    }
  });

  it("escapes control characters in the invalid-handle message", () => {
    let message = "";
    try {
      publicIdentity(ESCAPE_INPUT, "handle");
    } catch (error) {
      message = (error as Error).message;
    }
    expect(message).not.toMatch(CONTROL);
    expect(message).toMatch(/^\\u001b\]0;evil\\u0007 is not a valid handle\./);
    expect(invalidHandleMessage(ESCAPE_INPUT)).not.toMatch(CONTROL);
  });

  // a bidi override or isolate can reorder the error line in a terminal
  it.each([
    ["U+202E", "nick\u202Eeht", "nick\\u202eeht"],
    ["U+2066", "nick\u2066x\u2069", "nick\\u2066x\\u2069"],
  ])(
    "escapes bidi formatting characters in input with %s",
    (_name, input, shown) => {
      expect(() => publicIdentity(input, "handle")).toThrow(
        `${shown} is not a valid handle.`,
      );
      expect(invalidHandleMessage(input)).not.toMatch(BIDI);
    },
  );

  it("lowercases ASCII letters only", () => {
    // U+212A KELVIN SIGN lowercases to an ASCII k in JavaScript; the SDK
    // rejects it, so the CLI must not turn it into one.
    expect(asciiLowercase("NickK#2341")).toBe("nickK#2341");
  });

  it("sends a handle to the SDK as typed and keys it by its canonical text", () => {
    const identity = publicIdentity("Nick#2341", "handle");
    expect(identity).toEqual({ kind: "handle", identifier: "Nick#2341" });
    expect(canMessageKey(identity)).toBe("handle:nick#2341");
    expect(canMessageKey({ kind: "handle", identifier: "Nick\u212A" })).toBe(
      "handle:nick\u212A",
    );
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
