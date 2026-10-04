import {
  Client,
  MessageStream,
  Storage,
  XmtpError,
  initLogging,
  localSignerFromPrivateKey,
  setLogSink,
  type Signer,
} from "@xmtp/browser-sdk";
import { initPureWasm, TextCodec } from "@xmtp/browser-sdk/pure";
import { toBytes } from "viem";
import { generatePrivateKey, privateKeyToAccount } from "viem/accounts";

let client: Client | undefined;
const savedKey = localStorage.getItem("installed-signer");
const key = savedKey ? (savedKey as `0x${string}`) : generatePrivateKey();
localStorage.setItem("installed-signer", key);
const account = privateKeyToAccount(key);
const owner: Signer = {
  identity() {
    return Promise.resolve({
      kind: "ethereum" as const,
      identifier: account.address.toLowerCase(),
    });
  },
  kind() {
    return Promise.resolve({ kind: "eoa" as const });
  },
  async sign(request) {
    return {
      kind: "ecdsa",
      value: toBytes(await account.signMessage({ message: request.text })),
    };
  },
};
const storage = { location: { directory: "installed-package" } } as const;
const backend = { url: `${location.origin}/backend` };

export async function open(): Promise<void> {
  client = await Client.create(owner, {
    backend,
    storage,
    deviceSync: false,
    attachments: { allowPrivateNetwork: true },
  });
}
export async function busy(): Promise<boolean> {
  try {
    const other = await Client.create(owner, {
      backend,
      storage,
      deviceSync: false,
    });
    await other.end();
    return false;
  } catch (error) {
    return error instanceof XmtpError.StorageBusy;
  }
}
export async function exercise(): Promise<{
  sent: number;
  callbackCount: number;
  elapsedMs: number;
  attachmentBytes: number;
  standardEncodeCount: number;
  fallbackOverride: boolean;
  fallbackThrow: boolean;
}> {
  if (!client) throw new Error("Client missing");
  await initPureWasm();
  let standardEncodeCount = 0;
  class CountedText extends TextCodec {
    override encode(value: string) {
      standardEncodeCount++;
      return super.encode(value);
    }
  }
  const codec = new CountedText();
  const group = await client.conversations.createGroup([]);
  const messages = new Set<string>();
  const stream = MessageStream.open(client);
  const read = stream.onValue((message) => {
    if (message.conversationId === group.id) messages.add(message.id);
  });
  const start = performance.now();
  const ids: string[] = [];
  try {
    for (let index = 0; index < 20; index++)
      ids.push(
        await group.send(codec, `installed-${index}`, {
          shouldPush: true,
        }),
      );
    const deadline = Date.now() + 30_000;
    while (ids.some((id) => !messages.has(id))) {
      if (Date.now() >= deadline)
        throw new Error("Message callbacks did not arrive");
      await new Promise((resolve) => setTimeout(resolve, 20));
    }
    const values = await group.messages({ kind: "application" });
    if (
      values.length !== 20 ||
      values.some((message) => message.content.kind !== "text")
    )
      throw new Error("Installed messages changed");
    if (standardEncodeCount !== 20)
      throw new Error(
        `Standard codecs encoded ${standardEncodeCount} times for 20 sends`,
      );
    class OverrideText extends TextCodec {
      override fallback(value: string) {
        return `app ${value}`;
      }
    }
    const overrideId = await group.send(new OverrideText(), "fallback", {
      shouldPush: true,
    });
    if (
      (await client.conversations.getMessageById(overrideId))?.fallback !==
      "app fallback"
    )
      throw new Error("Pure codec fallback override was skipped");
    class FailedText extends TextCodec {
      override fallback(_value: string): string {
        throw new Error("app fallback failure");
      }
    }
    const before = await group.countMessages({ kind: "application" });
    let fallbackThrow = false;
    try {
      await group.send(new FailedText(), "must not publish", {
        shouldPush: true,
      });
    } catch (error) {
      fallbackThrow = error instanceof XmtpError.CodecEncodeFailed;
    }
    if (
      !fallbackThrow ||
      (await group.countMessages({ kind: "application" })) !== before
    )
      throw new Error("Pure codec fallback failure published");
    const bytes = new TextEncoder().encode("installed transfer through Rust");
    const pending = await client.attachments.create({
      kind: "bytes",
      bytes,
      mimeType: "text/plain",
      filename: "proof.txt",
    });
    await pending.upload();
    const remote = pending.remoteAttachment;
    await client.attachments.deleteLocal(remote);
    const downloaded = await client.attachments.download(remote);
    const parts = downloaded.path.split("/").filter(Boolean);
    let directory = await navigator.storage.getDirectory();
    for (const part of parts.slice(0, -1))
      directory = await directory.getDirectoryHandle(part);
    const file = await (await directory.getFileHandle(parts.at(-1)!)).getFile();
    const actual = new Uint8Array(await file.arrayBuffer());
    if (
      actual.length !== bytes.length ||
      actual.some((value, index) => value !== bytes[index])
    )
      throw new Error("Installed attachment bytes changed");
    return {
      standardEncodeCount,
      fallbackOverride: true,
      fallbackThrow,
      attachmentBytes: actual.length,
      sent: ids.length,
      callbackCount: ids.filter((id) => messages.has(id)).length,
      elapsedMs: performance.now() - start,
    };
  } finally {
    await stream.end();
    await read;
  }
}
export async function end(): Promise<void> {
  await client?.end();
  client = undefined;
}
export async function admin(): Promise<void> {
  const owner = await Storage.admin();
  try {
    await owner.listFiles();
  } finally {
    await owner.end();
  }
}

export async function callbackRejections(): Promise<
  Array<{ value: string; settled: boolean; typedSignerFailure: boolean }>
> {
  return Promise.all(
    [null, undefined].map(async (reason) => {
      const identity = privateKeyToAccount(generatePrivateKey());
      const rejecting: Signer = {
        identity() {
          return Promise.resolve({
            kind: "ethereum" as const,
            identifier: identity.address.toLowerCase(),
          });
        },
        kind() {
          return Promise.resolve({ kind: "eoa" as const });
        },
        sign() {
          // Reject a non-Error value to check the callback boundary.
          // oxlint-disable-next-line typescript/prefer-promise-reject-errors
          return Promise.reject(reason);
        },
      };
      let timer: ReturnType<typeof setTimeout> | undefined;
      try {
        const result = await Promise.race([
          Client.create(rejecting, {
            backend,
            storage: { location: "inMemory" },
            deviceSync: false,
          }).then(
            async (unexpected) => {
              await unexpected.end();
              return { settled: true, typedSignerFailure: false };
            },
            (error: unknown) => ({
              settled: true,
              typedSignerFailure:
                error instanceof XmtpError.Signer &&
                error.details.code === "SignerFailed" &&
                error.details.category === "callback" &&
                error.details.retryable === false,
            }),
          ),
          new Promise<{ settled: boolean; typedSignerFailure: boolean }>(
            (resolve) => {
              timer = setTimeout(
                () => resolve({ settled: false, typedSignerFailure: false }),
                8000,
              );
            },
          ),
        ]);
        return { value: String(reason), ...result };
      } finally {
        clearTimeout(timer);
      }
    }),
  );
}

async function waitForCount(
  read: () => number,
  expected: number,
): Promise<void> {
  const deadline = performance.now() + 8000;
  while (read() < expected) {
    if (performance.now() >= deadline)
      throw new Error(`Later callback missing: ${read()} of ${expected}`);
    await new Promise((resolve) => setTimeout(resolve, 20));
  }
}

// verifies: EVENT-051, LOG-009
export async function callbackVoidFailures(): Promise<{
  eventCount: number;
  logCount: number;
  messagingContinues: boolean;
}> {
  const current = await Client.create(owner, {
    backend,
    storage: { location: "inMemory" },
    deviceSync: false,
  });
  let eventCount = 0;
  let logCount = 0;
  let logInstalled = false;
  const listener = await current.startListener(
    { kinds: ["consent.changed"], references_own_messages: false },
    () => {
      eventCount++;
      if (eventCount <= 2) {
        // oxlint-disable-next-line typescript/prefer-promise-reject-errors
        return Promise.reject(eventCount === 1 ? null : undefined);
      }
      return Promise.resolve();
    },
  );
  try {
    const group = await current.conversations.createGroup([]);
    for (const [index, state] of (
      ["denied", "allowed", "denied"] as const
    ).entries()) {
      await current.preferences.setConsentStates([
        { entity: { kind: "conversation", conversationId: group.id }, state },
      ]);
      await waitForCount(() => eventCount, index + 1);
    }
    await initLogging({ level: "error" });
    await setLogSink({
      log() {
        logCount++;
        if (logCount <= 2) {
          // oxlint-disable-next-line typescript/prefer-promise-reject-errors
          return Promise.reject(logCount === 1 ? null : undefined);
        }
        return Promise.resolve();
      },
    });
    logInstalled = true;
    for (let index = 0; index < 3; index++) {
      await localSignerFromPrivateKey(new Uint8Array(31)).then(
        () => {
          throw new Error("Invalid signer key was accepted");
        },
        () => undefined,
      );
      await waitForCount(() => logCount, index + 1);
    }
    const sent = await group.sendText("after callback failures", {
      shouldPush: false,
    });
    const stored = await current.conversations.getMessageById(sent);
    return { eventCount, logCount, messagingContinues: stored?.id === sent };
  } finally {
    await current.stopListener(listener);
    if (logInstalled) await setLogSink();
    await current.end();
  }
}
