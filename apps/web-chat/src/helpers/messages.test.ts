import type { Message, MessageContent } from "@xmtp/browser-sdk";
import { describe, expect, it } from "vitest";

import { stringify } from "./messages";

const message = (content: MessageContent, fallback?: string) =>
  ({ content, fallback }) as Message;

describe("stringify", () => {
  it("shows text and markdown", () => {
    expect(
      stringify(message({ kind: "text", value: "hello" }, "fallback")),
    ).toBe("hello");
    expect(stringify(message({ kind: "markdown", value: "**hello**" }))).toBe(
      "**hello**",
    );
  });
  it("shows nested reply text", () => {
    expect(
      stringify(
        message({
          kind: "reply",
          referenceId: "parent",
          body: { kind: "text", value: "reply" },
        }),
      ),
    ).toBe("reply");
  });
  it("shows a reaction", () => {
    expect(
      stringify(
        message({
          kind: "reaction",
          reference: "parent",
          reaction: { action: "added", schema: "unicode", content: "👍" },
        }),
      ),
    ).toBe("👍");
  });
  it("uses fallback for unknown content", () => {
    expect(
      stringify(
        message(
          {
            kind: "unknown",
            rawBytes: new Uint8Array(),
            error: {
              code: "MalformedEnvelope",
              category: "input",
              retryable: false,
              message: "Invalid content",
            },
          },
          "fallback",
        ),
      ),
    ).toBe("fallback");
  });
});
