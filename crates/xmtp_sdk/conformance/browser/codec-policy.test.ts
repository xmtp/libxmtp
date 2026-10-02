import { runInNewContext } from "node:vm";

import { afterEach, beforeAll, beforeEach, describe, expect, it } from "vitest";

import {
  initPureWasm,
  TextCodec,
} from "../../../../target/sdk-generated/typescript-pure/index";
import * as P from "../../../../target/sdk-generated/typescript-wasm/public-values.gen";
import type { ContentCodec } from "../../../../target/sdk-generated/typescript-wasm/runtime/public/codec";
import {
  codecType,
  contentForSend,
  encodeForSend,
  optionsForSend,
} from "../../../../target/sdk-generated/typescript-wasm/runtime/public/codec-policy";
import { Message } from "../../../../target/sdk-generated/typescript-wasm/runtime/public/message";
import type * as B from "../../../../target/sdk-generated/typescript-wasm/xmtp_sdk";

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
function codec(
  steps: Partial<ContentCodec<string>> = {},
): ContentCodec<string> {
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
      envelope({ type: { ...noteType, versionMajor: 2 ** 32 } }),
      envelope({ content: [1] }),
      envelope({ parameters: new Map([["k", 1]]) }),
      envelope({ parameters: { k: "v" } }),
      null,
    ])
      codecEncodeFailed(() =>
        encodeForSend(codec({ encode: () => bad as P.EncodedContent }), "x"),
      );
  });

  // verifies: CTYPE-003
  it("rejects an empty authority or type ID", () => {
    // The codec and its envelope agree, so only the empty identifier rejects
    // it, before the binding sees the envelope.
    for (const empty of [
      { ...noteType, authorityId: "" },
      { ...noteType, typeId: "" },
    ])
      codecEncodeFailed(() =>
        encodeForSend(
          codec({ type: empty, encode: () => envelope({ type: empty }) }),
          "x",
        ),
      );
  });

  it("keeps a failing getter or thenable check inside the failure boundary", () => {
    const throwingThen = Object.defineProperty(envelope(), "then", {
      get() {
        throw new Error("then getter");
      },
    });
    codecEncodeFailed(() =>
      encodeForSend(codec({ encode: () => throwingThen }), "x"),
    );
    const throwingType = Object.defineProperty(
      { content: new Uint8Array([1]) },
      "type",
      {
        get() {
          throw new Error("type getter");
        },
      },
    );
    codecEncodeFailed(() =>
      encodeForSend(
        codec({ encode: () => throwingType as P.EncodedContent }),
        "x",
      ),
    );
  });

  it("passes a value that is not an object through as an envelope", () => {
    // The binding, not the codec policy, rejects it with its input error.
    for (const content of [null, "text"]) {
      const [sent] = contentForSend(content as never, undefined, undefined);
      expect(sent).toBe(content);
    }
  });

  it("accepts envelope bytes and parameters from another realm", () => {
    const foreign = runInNewContext(
      "({ bytes: new Uint8Array([1, 2]), parameters: new Map([['k', 'v']]) })",
    ) as { bytes: Uint8Array; parameters: Map<string, string> };
    expect(foreign.bytes instanceof Uint8Array).toBe(false);
    const sent = encodeForSend(
      codec({
        encode: () =>
          ({
            type: noteType,
            content: foreign.bytes,
            parameters: foreign.parameters,
          }) as P.EncodedContent,
      }),
      "x",
    );
    expect(sent.content).toBe(foreign.bytes);
    expect([...(sent.parameters ?? new Map())]).toEqual([["k", "v"]]);
    // A value that only claims to be bytes or a map is rejected.
    codecEncodeFailed(() =>
      encodeForSend(
        codec({
          encode: () =>
            envelope({ content: { [Symbol.toStringTag]: "Uint8Array" } }),
        }),
        "x",
      ),
    );
    codecEncodeFailed(() =>
      encodeForSend(
        codec({
          encode: () =>
            envelope({ parameters: { [Symbol.toStringTag]: "Map" } }),
        }),
        "x",
      ),
    );
  });

  it("keeps a throwing codec check inside the failure boundary", () => {
    const trap = new Proxy(codec(), {
      has() {
        throw new Error("has trap");
      },
    });
    codecEncodeFailed(() => contentForSend(trap, "x", undefined));
  });

  it("describes a failure whose message or text cannot be read", () => {
    const unreadable = Object.defineProperty(new Error(), "message", {
      get() {
        throw new Error("message getter");
      },
    });
    const noText = {
      toString() {
        throw new Error("toString");
      },
    };
    for (const cause of [unreadable, noText])
      codecEncodeFailed(() =>
        encodeForSend(
          codec({
            encode: () => {
              throw cause;
            },
          }),
          "x",
        ),
      );
  });

  it("does not read a skipped hook", () => {
    const throwing = (step: string) => ({
      get() {
        throw new Error(`${step} member must not be read`);
      },
    });
    // An envelope with its own fallback skips the fallback hook.
    const own = Object.defineProperty(
      codec({ encode: () => envelope({ fallback: "own" }) }),
      "fallback",
      throwing("fallback"),
    );
    expect(encodeForSend(own, "x").fallback).toBe("own");
    const notAFunction = codec({
      encode: () => envelope({ fallback: "own" }),
      fallback: 7 as unknown as () => string,
    });
    expect(encodeForSend(notAFunction, "x").fallback).toBe("own");
    // An explicit option and a catalogue type skip the push hook.
    const push = Object.defineProperty(
      codec(),
      "shouldPush",
      throwing("shouldPush"),
    );
    expect(optionsForSend(push, "x", { shouldPush: false }, custom)).toEqual({
      shouldPush: false,
    });
    expect(optionsForSend(push, "x", undefined, catalogue)).toBeUndefined();
    const invalidPush = codec({ shouldPush: 7 as unknown as () => boolean });
    expect(
      optionsForSend(invalidPush, "x", undefined, catalogue),
    ).toBeUndefined();
  });

  it("rejects a content type version outside u32", () => {
    // The codec and its envelope agree, so only the u32 bound rejects it.
    const wide = { ...noteType, versionMajor: 2 ** 32 };
    codecEncodeFailed(() =>
      encodeForSend(
        codec({ type: wide, encode: () => envelope({ type: wide }) }),
        "x",
      ),
    );
  });

  // verifies: CTYPE-007
  it("rejects an envelope of another type than the codec", () => {
    // A custom codec cannot send a catalogue type, so its push hook cannot
    // steer catalogue dispatch.
    const readReceipt = {
      authorityId: "xmtp.org",
      typeId: "readReceipt",
      versionMajor: 1,
      versionMinor: 0,
    };
    codecEncodeFailed(() =>
      encodeForSend(
        codec({ encode: () => envelope({ type: readReceipt }) }),
        "x",
      ),
    );
    codecEncodeFailed(() =>
      encodeForSend(
        codec({
          encode: () => envelope({ type: { ...noteType, versionMinor: 1 } }),
        }),
        "x",
      ),
    );
  });

  it("calls each step on its codec, so a class codec can use this", () => {
    class NoteCodec implements ContentCodec<string> {
      readonly type = noteType;
      readonly prefix = "note";
      encode(value: string): P.EncodedContent {
        return envelope({
          content: new TextEncoder().encode(`${this.prefix}:${value}`),
        });
      }
      decode(): string {
        return this.prefix;
      }
      fallback(value: string): string {
        return `${this.prefix} ${value}`;
      }
      shouldPush(value: string): boolean {
        return this.prefix === "note" && value === "loud";
      }
    }
    const note = new NoteCodec();
    expect(encodeForSend(note, "x").fallback).toBe("note x");
    expect(optionsForSend(note, "loud", undefined, custom)).toEqual({
      shouldPush: true,
    });
  });

  // verifies: SEND-021
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

class TestProjection extends P.ObjectProjection {
  liftMessage(): never {
    throw new Error("no message in these calls");
  }

  lowerMessage(): never {
    throw new Error("no message in these calls");
  }
}

describe("typed codec sends on a Group (Decisions 23 and 24)", () => {
  // The catalogue predicate is a pure WASM function on the main thread.
  beforeAll(async () => {
    await initPureWasm();
    P.installProjection(new TestProjection());
  });

  it("checks a snapshot, so a hook cannot change the checked envelope", () => {
    const readReceipt = {
      authorityId: "xmtp.org",
      typeId: "readReceipt",
      versionMajor: 1,
      versionMinor: 0,
    };
    const kept = { type: { ...noteType }, content: new Uint8Array([1]) };
    const changing = codec({
      encode: () => kept,
      fallback: () => {
        // The codec changes the envelope object that it returned.
        Object.assign(kept.type, readReceipt);
        return "a note";
      },
    });
    const [sent] = contentForSend(changing, "x", undefined);
    expect(sent.type).toEqual(noteType);
    expect(sent.fallback).toBe("a note");
  });

  it("reads the codec type once, inside the failure boundary", () => {
    let reads = 0;
    const counted = {
      ...codec({ shouldPush: () => true }),
      get type() {
        reads += 1;
        return noteType;
      },
    };
    contentForSend(counted, "x", undefined);
    expect(reads).toBe(1);
    const throwing = {
      ...codec(),
      get type(): P.ContentTypeId {
        throw new Error("type getter");
      },
    };
    codecEncodeFailed(() => codecType(throwing));
    codecEncodeFailed(() => contentForSend(throwing, "x", undefined));
  });

  function recordingGroup(calls: [string, unknown, unknown][]): P.Group {
    const record =
      (name: string) => async (encoded: unknown, options: unknown) => {
        calls.push([name, encoded, options]);
        return "id";
      };
    return P.wrapGroup(
      binding({
        send: record("send"),
        prepareMessage: record("prepareMessage"),
      }),
    );
  }
  const binding = (fields: object): B.GroupLike => fields as B.GroupLike;
  const pushOf = (options: unknown) =>
    (options as { shouldPush?: boolean } | undefined)?.shouldPush;

  it("encodes callable codecs before send and prepareMessage", async () => {
    const calls: [string, unknown, unknown][] = [];
    const values: string[] = [];
    const callable: ContentCodec<string> = Object.assign(
      () => undefined,
      codec({
        encode: (value) => {
          values.push(value);
          return envelope({ content: new TextEncoder().encode(value) });
        },
        fallback: (value) => `about ${value}`,
        shouldPush: () => false,
      }),
    );
    const group = recordingGroup(calls);
    await group.send(callable, "sent");
    await group.prepareMessage(callable, "prepared");
    expect(values).toEqual(["sent", "prepared"]);
    expect(calls.map(([name]) => name)).toEqual(["send", "prepareMessage"]);
    for (const [index, [, encoded, options]] of calls.entries()) {
      expect(
        new TextDecoder().decode(
          new Uint8Array((encoded as B.EncodedContent).content),
        ),
      ).toBe(values[index]);
      expect((encoded as B.EncodedContent).fallback).toBe(
        `about ${values[index]}`,
      );
      expect(pushOf(options)).toBe(false);
    }
  });

  it("encodes a callable codec before a reply", async () => {
    const replies: [string, P.EncodedContent, P.SendOptions | undefined][] = [];
    const parent = Object.assign(Object.create(Message.prototype) as Message, {
      id: "parent-id",
      client: () => ({
        conversations: {
          replyToMessage: async (
            id: string,
            encoded: P.EncodedContent,
            options?: P.SendOptions,
          ) => {
            replies.push([id, encoded, options]);
            return "reply-id";
          },
        },
      }),
    });
    const callable: ContentCodec<string> = Object.assign(
      () => undefined,
      codec({
        encode: (value) =>
          envelope({ content: new TextEncoder().encode(value) }),
        fallback: (value) => `about ${value}`,
        shouldPush: never("reply push"),
      }),
    );
    expect(await parent.reply(callable, "reply")).toBe("reply-id");
    expect(replies).toHaveLength(1);
    const [id, encoded, options] = replies[0]!;
    expect(id).toBe("parent-id");
    expect(new TextDecoder().decode(encoded.content)).toBe("reply");
    expect(encoded.fallback).toBe("about reply");
    expect(options).toBeUndefined();
  });

  it("sends the codec's envelope with the push the policy chose", async () => {
    const calls: [string, unknown, unknown][] = [];
    const group = recordingGroup(calls);
    const quiet = codec({
      fallback: (value) => `about ${value}`,
      shouldPush: () => false,
    });
    await group.send(quiet, "x");
    await group.prepareMessage(quiet, "y");
    expect(calls.map(([name]) => name)).toEqual(["send", "prepareMessage"]);
    for (const [, encoded, options] of calls) {
      expect((encoded as { fallback?: string }).fallback).toMatch(/^about /);
      expect(pushOf(options)).toBe(false);
    }
  });

  // verifies: SEND-021
  it("keeps an explicit push, a catalogue default, and an envelope send", async () => {
    const calls: [string, unknown, unknown][] = [];
    const group = recordingGroup(calls);
    const throwingPush = codec({ shouldPush: never("shouldPush") });
    await group.send(throwingPush, "x", { shouldPush: true });
    expect(pushOf(calls[0]![2])).toBe(true);
    // A codec of a catalogue type keeps the catalogue default: no hook call,
    // no push option.
    const text = {
      authorityId: "xmtp.org",
      typeId: "text",
      versionMajor: 1,
      versionMinor: 0,
    };
    const catalogueCodec = codec({
      type: text,
      encode: () => envelope({ type: text }),
      shouldPush: never("shouldPush"),
    });
    await group.send(catalogueCodec, "x");
    expect(pushOf(calls[1]![2])).toBeUndefined();
    await group.send(envelope(), { shouldPush: false });
    expect(pushOf(calls[2]![2])).toBe(false);
  });

  // verifies: CTYPE-031
  it("adds no compression unless the send asks for it", async () => {
    const calls: [string, unknown, unknown][] = [];
    const group = recordingGroup(calls);
    const pushing = codec({ shouldPush: () => true });
    await group.send(pushing, "x");
    await group.send(pushing, "y", { compression: "gzip" });
    await group.send(codec(), "z");
    const compressionOf = (options: unknown) =>
      (options as { compression?: unknown } | undefined)?.compression;
    expect(compressionOf(calls[0]![2])).toBeUndefined();
    expect(compressionOf(calls[1]![2])).toBeDefined();
    expect(calls[2]![2]).toBeUndefined();
  });

  it("honours the push hook of an app codec for legacy reaction v1", async () => {
    // Reaction v1 is outside the catalogue (CTYPE section 6), so its codec's
    // shouldPush decides.
    const calls: [string, unknown, unknown][] = [];
    const group = recordingGroup(calls);
    const reactionV1 = {
      authorityId: "xmtp.org",
      typeId: "reaction",
      versionMajor: 1,
      versionMinor: 0,
    };
    await group.send(
      codec({
        type: reactionV1,
        encode: () => envelope({ type: reactionV1 }),
        shouldPush: () => false,
      }),
      "x",
    );
    expect(pushOf(calls[0]![2])).toBe(false);
  });

  it("preserves a standard TextCodec subclass fallback override", async () => {
    class CustomText extends TextCodec {
      override fallback(value: string): string {
        return `custom ${value}`;
      }
    }
    const calls: [string, unknown, unknown][] = [];
    await recordingGroup(calls).send(new CustomText(), "text");
    expect((calls[0]![1] as { fallback?: string }).fallback).toBe(
      "custom text",
    );
  });

  it("stops before send when a standard TextCodec subclass fallback throws", async () => {
    class FailedText extends TextCodec {
      override fallback(_value: string): string {
        throw new Error("app fallback");
      }
    }
    const calls: [string, unknown, unknown][] = [];
    await expect(
      recordingGroup(calls).send(new FailedText(), "text"),
    ).rejects.toBeInstanceOf(P.XmtpError.CodecEncodeFailed);
    expect(calls).toEqual([]);
  });

  it("makes no send call when a codec step fails", async () => {
    const calls: [string, unknown, unknown][] = [];
    const group = recordingGroup(calls);
    for (const failing of [
      codec({ encode: never("encode") }),
      codec({ fallback: never("fallback") }),
      codec({ shouldPush: never("shouldPush") }),
    ]) {
      await expect(group.send(failing, "x")).rejects.toBeInstanceOf(
        P.XmtpError.CodecEncodeFailed,
      );
      await expect(group.prepareMessage(failing, "x")).rejects.toBeInstanceOf(
        P.XmtpError.CodecEncodeFailed,
      );
    }
    expect(calls).toEqual([]);
  });
});
