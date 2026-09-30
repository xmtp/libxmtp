import { afterEach, beforeEach, describe, expect, it } from "vitest";

import * as P from "../../../../target/sdk-generated/typescript-wasm/public-values.gen";
import type { ContentCodec } from "../../../../target/sdk-generated/typescript-wasm/runtime/public/codec";
import {
  encodeForSend,
  optionsForSend,
} from "../../../../target/sdk-generated/typescript-wasm/runtime/public/codec-policy";

// The typed codec send policy (Ref Public surface, Host codecs; P10). Every
// failed step is CodecEncodeFailed, and a rejected async step is handled.
const noteType: P.ContentTypeId = {
  authorityId: "example.org",
  typeId: "note",
  versionMajor: 1,
  versionMinor: 0,
};
const envelope = (fields: object = {}): P.EncodedContent => ({
  type: noteType,
  content: new Uint8Array([1]),
  ...fields,
});
function codec(steps: Partial<ContentCodec<string>> = {}): ContentCodec<string> {
  return {
    type: noteType,
    encode: () => envelope(),
    decode: () => "note",
    ...steps,
  };
}
const custom = () => false;
const catalogue = () => true;
const never = (step: string) => () => {
  throw new Error(`${step} must not run`);
};

function codecEncodeFailed(run: () => unknown): void {
  let failure: unknown;
  try {
    run();
  } catch (error) {
    failure = error;
  }
  expect(failure).toBeInstanceOf(P.XmtpError.CodecEncodeFailed);
  expect(P.XmtpError.CodecEncodeFailed).toBeDefined();
  if (!(failure instanceof P.XmtpError)) throw new Error("no public error");
  expect(failure.details).toMatchObject({
    code: "CodecEncodeFailed",
    category: "callback",
    retryable: false,
  });
}

const unhandled: unknown[] = [];
const record = (reason: unknown) => void unhandled.push(reason);
beforeEach(() => {
  unhandled.length = 0;
  process.on("unhandledRejection", record);
});
afterEach(() => {
  process.off("unhandledRejection", record);
});
// Let a rejected Promise that nobody handled reach the process.
const settle = () => new Promise((resolve) => setTimeout(resolve, 20));

describe("typed codec send policy", () => {
  it("fills a missing fallback and keeps an envelope's own", () => {
    const hook = codec({ fallback: (value) => `about ${value}` });
    expect(encodeForSend(hook, "x").fallback).toBe("about x");
    const own = codec({
      encode: () => envelope({ fallback: "own" }),
      fallback: never("fallback"),
    });
    expect(encodeForSend(own, "x").fallback).toBe("own");
    expect(encodeForSend(codec(), "x").fallback).toBeUndefined();
  });

  it("rejects an envelope with the wrong shape", () => {
    for (const bad of [
      envelope({ fallback: 7 }),
      envelope({ type: null }),
      envelope({ type: { authorityId: "example.org", typeId: "note" } }),
      envelope({ type: { ...noteType, versionMajor: -1 } }),
      envelope({ content: [1] }),
      envelope({ parameters: new Map([["k", 1]]) }),
      envelope({ parameters: { k: "v" } }),
      null,
    ])
      codecEncodeFailed(() =>
        encodeForSend(codec({ encode: () => bad as P.EncodedContent }), "x"),
      );
  });

  it("chooses push: explicit option, then catalogue, then the hook", () => {
    const hook = codec({ shouldPush: (value) => value === "loud" });
    expect(optionsForSend(hook, "loud", undefined, custom)).toEqual({
      shouldPush: true,
    });
    expect(optionsForSend(hook, "quiet", undefined, custom)).toEqual({
      shouldPush: false,
    });
    const skipped = codec({ shouldPush: never("shouldPush") });
    expect(optionsForSend(skipped, "x", { shouldPush: false }, custom)).toEqual(
      { shouldPush: false },
    );
    expect(optionsForSend(skipped, "x", undefined, catalogue)).toBeUndefined();
    expect(optionsForSend(codec(), "x", undefined, custom)).toBeUndefined();
    codecEncodeFailed(() =>
      optionsForSend(codec({ shouldPush: never("x") }), "x", undefined, custom),
    );
    codecEncodeFailed(() =>
      optionsForSend(
        codec({ shouldPush: () => "yes" as unknown as boolean }),
        "x",
        undefined,
        custom,
      ),
    );
  });

  it("handles a rejected async step as CodecEncodeFailed", async () => {
    const rejected = () => Promise.reject(new Error("async step rejected"));
    codecEncodeFailed(() =>
      encodeForSend(
        codec({ encode: rejected as unknown as () => P.EncodedContent }),
        "x",
      ),
    );
    codecEncodeFailed(() =>
      encodeForSend(
        codec({ fallback: rejected as unknown as () => string }),
        "x",
      ),
    );
    codecEncodeFailed(() =>
      optionsForSend(
        codec({ shouldPush: rejected as unknown as () => boolean }),
        "x",
        undefined,
        custom,
      ),
    );
    await settle();
    expect(unhandled).toEqual([]);
  });
});
