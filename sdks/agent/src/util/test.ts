import {
  type ContentCodec,
  type ContentTypeId,
  type EncodedContent,
} from "@xmtp/content-type-primitives";
import {
  Client,
  generateInboxId,
  type ClientOptions,
  type NetworkOptions,
} from "@xmtp/node-sdk";
import { vi } from "vitest";

import { createSigner, createUser } from "@/user/User";

/**
 * How long a network-backed condition may take to become true, and how often to
 * re-check it.
 *
 * `vi.waitFor` defaults to a 1000 ms timeout with a 50 ms interval, and it does
 * *not* inherit `testTimeout` from `vitest.config.ts`. Both defaults are wrong
 * here: every wait in this suite drives real MLS work against a live backend —
 * conversation sync, DM creation, message send — and a single `sync()` can
 * exceed a second on a loaded runner. The 50 ms interval makes it worse by
 * re-running `sync()` twenty times a second, adding the very load it races.
 *
 * 30 s matches the budget `createConversationAndWait` already uses, and stays
 * under `testTimeout` so a stuck wait fails with the condition's own assertion
 * rather than as an opaque whole-test timeout.
 */
export const NETWORK_WAIT = { timeout: 30_000, interval: 250 } as const;

/**
 * Waits for a condition backed by real network work, retrying until it stops
 * throwing.
 *
 * Use this instead of a bare `vi.waitFor` for anything that awaits the backend.
 */
export const waitForNetwork = <T>(condition: () => T | Promise<T>) =>
  vi.waitFor(condition, NETWORK_WAIT);

export const createClient = async <ContentCodecs extends ContentCodec[] = []>(
  options?: Omit<ClientOptions & NetworkOptions, "codecs" | "backendUrl"> &
    Partial<NetworkOptions> & {
      codecs?: ContentCodecs;
    },
) => {
  const backendUrl = options?.backendUrl ?? process.env.XMTP_BACKEND_URL;
  if (!backendUrl) throw new Error("XMTP_BACKEND_URL is required");
  const signer = createSigner(createUser());
  const identifier = await signer.getIdentifier();
  const inboxId = generateInboxId(identifier);

  let dbPath: string;
  if (typeof options?.dbPath === "function") {
    dbPath = options.dbPath(inboxId);
  } else {
    dbPath = options?.dbPath ?? `./test-${inboxId}.db3`;
  }

  return Client.create<ContentCodecs>(signer, {
    backendUrl,
    ...options,
    dbPath,
    disableDeviceSync: true,
    env: "local",
  });
};

export const createConversationAndWait = async <
  ContentTypes,
  Created extends { id: string },
>(
  recipient: Client<ContentTypes>,
  create: () => Promise<Created>,
) => {
  let reportError!: (failure: { error: Error }) => void;
  const streamError = new Promise<{ error: Error }>((resolve) => {
    reportError = resolve;
  });
  // Subscribe before creation so the new Welcome is observed.
  const stream = await recipient.conversations.stream({
    retryOnFail: false,
    onError: (error) => reportError({ error }),
  });
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    const created = await create();
    const arrival = (async () => {
      for await (const received of stream) {
        if (received.id === created.id) return { received };
      }
      throw new Error(`Conversation stream ended before ${created.id} arrived`);
    })();
    const timeout = new Promise<{ error: Error }>((resolve) => {
      timer = setTimeout(() => {
        resolve({
          error: new Error(`Conversation ${created.id} did not arrive in 30s`),
        });
      }, 30_000);
    });
    const result = await Promise.race([arrival, streamError, timeout]);
    if ("error" in result) throw result.error;
    return { created, received: result.received };
  } finally {
    clearTimeout(timer);
    await stream.end();
  }
};

export const ContentTypeTest: ContentTypeId = {
  authorityId: "xmtp.org",
  typeId: "test",
  versionMajor: 1,
  versionMinor: 0,
};

export class TestCodec implements ContentCodec {
  contentType = ContentTypeTest;
  encode(content: Record<string, string>): EncodedContent {
    return {
      type: this.contentType,
      parameters: {},
      content: new TextEncoder().encode(JSON.stringify(content)),
    };
  }
  decode(content: EncodedContent): Record<string, string> {
    const decoded = new TextDecoder().decode(content.content);
    // oxlint-disable-next-line typescript/no-unsafe-return
    return JSON.parse(decoded);
  }
  fallback() {
    return undefined;
  }
  shouldPush() {
    return false;
  }
}
