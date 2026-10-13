import { XmtpError, type ErrorDetails } from "@xmtp/browser-sdk";
import { describe, expect, it } from "vitest";

import {
  checkHandleInput,
  claimFailure,
  handleHint,
  isValidHandle,
  splitHandle,
} from "./handles";

describe("checkHandleInput", () => {
  it.each([
    ["nick.eth", "Use only lowercase letters, numbers, _ and -."],
    ["1nick", "Start with a letter."],
    ["#1234", "Start with a letter."],
    ["nick#12", "After #, use exactly 4 digits, like nick#0012."],
    ["nick#12345", "After #, use exactly 4 digits, like nick#0012."],
    ["nicholas_the_magnificent_one", "Use 27 characters or fewer before #."],
  ])("gives the design message for %s", (input, message) => {
    expect(checkHandleInput(input, 3)).toBe(message);
  });

  it("applies the published minimum to the name part only", () => {
    expect(checkHandleInput("jo", 3)).toBe(
      "Use at least 3 characters on this server.",
    );
    expect(checkHandleInput("jo#1234", 3)).toBe(
      "Use at least 3 characters on this server.",
    );
    expect(checkHandleInput("jo", 2)).toBeNull();
  });

  it("accepts every input that the handle grammar accepts", () => {
    for (const input of ["a", "nick", "nick#0000", "n_-9", "a".repeat(27)]) {
      expect(checkHandleInput(input, 1)).toBeNull();
      expect(isValidHandle(input)).toBe(true);
    }
    expect(checkHandleInput("", 3)).toBeNull();
  });
});

it("splits a handle into its name part and code", () => {
  expect(splitHandle("nick#2341")).toEqual({ name: "nick", code: "2341" });
  expect(splitHandle("nick")).toEqual({ name: "nick", code: undefined });
  expect(handleHint(5)).toBe("5 to 27 lowercase letters, numbers, _ or -.");
});

describe("claimFailure", () => {
  const details = (code: string): ErrorDetails => ({
    code,
    category: "identity",
    retryable: false,
    message: code,
  });

  it("maps each SDK handle error to the answer that the app shows", () => {
    expect(
      claimFailure(new XmtpError.HandleTaken(details("HandleTaken"))),
    ).toBe("taken");
    expect(
      claimFailure(new XmtpError.HandleNotAllowed(details("HandleNotAllowed"))),
    ).toBe("notAllowed");
    expect(
      claimFailure(new XmtpError.HandleTooSoon(details("HandleTooSoon"))),
    ).toBe("tooSoon");
    expect(
      claimFailure(
        new XmtpError.HandleLimitReached(details("HandleLimitReached")),
      ),
    ).toBe("limitReached");
    expect(claimFailure(new Error("offline"))).toBe("failed");
  });
});
