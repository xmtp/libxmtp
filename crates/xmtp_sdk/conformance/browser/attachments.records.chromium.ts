// Attachment failure records and worker death, in the conformance-featured
// panic fixture. Its hooks fail an operation with any record.
// @ts-ignore The browser fixture uses the published JavaScript build of viem.
import { generatePrivateKey } from "../../../../sdks/browser/node_modules/viem/_esm/accounts/generatePrivateKey.js";
// @ts-ignore The browser fixture uses the published JavaScript build of viem.
import { privateKeyToAccount } from "../../../../sdks/browser/node_modules/viem/_esm/accounts/privateKeyToAccount.js";
// @ts-ignore The browser fixture uses the published JavaScript build of viem.
import { toBytes } from "../../../../sdks/browser/node_modules/viem/_esm/utils/encoding/toBytes.js";
import {
  CONTRACT_HASH,
  PROTOCOL_VERSION,
} from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/contract.gen";
import * as fx from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/index";
import {
  bridgeTestPanic,
  Client as ProxyClient,
  sdkConformanceAttachmentError,
} from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/proxy.gen";
import { wrapClient } from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/public-client.gen";
import {
  currentProjection,
  lowerAttachmentFailure,
  lowerSigner,
  publicError,
} from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/public-values.gen";
import { MainSession } from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/runtime/bridge/main/session";
import type {
  WireEndpoint,
  WireMessage,
} from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/runtime/bridge/wire";
import {
  hostOptions,
  publicClient,
} from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/runtime/public/client";
import {
  attachmentFilter,
  bytesSource,
  drain,
  failure,
  same,
  within,
} from "./attachments-support";
import { equal, expect } from "./suite-support";

function connection(): { session: MainSession; worker: Worker } {
  const worker = new Worker(
    new URL("./attachments.records.worker.ts", import.meta.url),
    { type: "module" },
  );
  const endpoint: WireEndpoint = {
    postMessage(message, transfer) {
      worker.postMessage(message, { transfer });
    },
    onMessage(handler) {
      worker.addEventListener("message", (event: MessageEvent<WireMessage>) =>
        handler(event.data),
      );
    },
    onExit(handler) {
      worker.addEventListener("error", handler);
    },
    terminate() {
      worker.terminate();
    },
  };
  return {
    worker,
    session: new MainSession(endpoint, PROTOCOL_VERSION, CONTRACT_HASH),
  };
}

async function create(
  session: MainSession,
  options: fx.ClientOptions,
): Promise<fx.Client> {
  const account = privateKeyToAccount(generatePrivateKey());
  const signer: fx.Signer = {
    async identity() {
      return { kind: "ethereum", identifier: account.address.toLowerCase() };
    },
    async kind() {
      return { kind: "eoa" };
    },
    async sign(request) {
      const signed = await account.signMessage({ message: request.text });
      return { kind: "ecdsa", value: Uint8Array.from(toBytes(signed)) };
    },
  };
  const projection = currentProjection();
  const proxy = await ProxyClient.create(
    session,
    lowerSigner(signer, projection),
    hostOptions(options, projection),
  ).catch((error: unknown) => {
    throw publicError(error);
  });
  return publicClient(wrapClient(proxy));
}

async function thrownError(
  action: Promise<unknown>,
): Promise<InstanceType<typeof fx.XmtpError.Attachment>> {
  try {
    await action;
  } catch (error) {
    expect(
      error instanceof fx.XmtpError.Attachment,
      `expected an attachment error, got ${String(error)}`,
    );
    equal(error.details.code, "Attachment", "attachment error code");
    return error;
  }
  throw new Error("expected an attachment error, got success");
}

/** A call on a client whose worker died fails closed. */
async function rejectsClosed(
  action: Promise<unknown>,
  label: string,
): Promise<void> {
  try {
    await within(action, label);
  } catch (error) {
    expect(
      error instanceof fx.XmtpError.ClientClosed &&
        error.details.category === "lifecycle",
      `${label}: expected ClientClosed, got ${String(error)}`,
    );
    return;
  }
  throw new Error(`${label} succeeded after the worker died`);
}

/** The public form of a call in flight when the worker died. */
async function rejectsWorkerDeath(
  action: Promise<unknown>,
  label: string,
): Promise<void> {
  try {
    await within(action, label);
  } catch (error) {
    expect(
      error instanceof fx.XmtpError.Unknown &&
        error.details.category === "lifecycle" &&
        !error.details.retryable &&
        error.details.message === "workerTerminated",
      `${label}: expected the worker failure, got ${String(error)}`,
    );
    return;
  }
  throw new Error(`${label} succeeded after the worker died`);
}

// Each cause with the error category and retry the ATCH table gives it.
const FAILURE_TABLE: [fx.AttachmentFailure, fx.ErrorCategory, boolean][] = [
  [failure("notOffered"), "configuration", false],
  [failure("tooLarge"), "input", false],
  [failure("sourceUnreadable"), "input", false],
  [failure("localStorage"), "storage", true],
  [failure("stagedUnusable"), "storage", false],
  [failure("connectionBlocked"), "configuration", false],
  [
    failure("credential", {
      credentialKind: "credentialRejected",
      missingScope: true,
    }),
    "callback",
    false,
  ],
  [
    failure("credential", { credentialKind: "callbackFailed", retryable: true }),
    "callback",
    true,
  ],
  [failure("credential", { credentialKind: "exhausted" }), "callback", false],
  [
    failure("credential", { credentialKind: "missingCredential" }),
    "callback",
    false,
  ],
  [failure("backendRejected"), "network", false],
  [failure("backendUnavailable"), "network", true],
  [failure("targetRejected", { httpStatus: 403 }), "network", true],
  [failure("network"), "network", true],
  [failure("insecureUrl"), "input", false],
  [failure("blockedAddress"), "network", false],
  [failure("tooManyRedirects"), "network", false],
  [failure("notFound", { httpStatus: 404 }), "network", true],
  [failure("httpStatus", { httpStatus: 408 }), "network", true],
  [failure("httpStatus", { httpStatus: 429 }), "network", true],
  [failure("httpStatus", { httpStatus: 503 }), "network", true],
  [failure("httpStatus", { httpStatus: 403 }), "network", false],
  [failure("malformed"), "input", false],
  [failure("digestMismatch"), "input", false],
  [failure("decryptionFailed"), "input", false],
  [failure("notAnAttachment"), "input", false],
  [failure("deleted"), "storage", true],
];

/**
 * Every cause and credential kind, thrown and recorded; no resend of a
 * terminal rejection; and the calls a worker's death settles.
 */
export async function checkAttachmentRecords(
  backendURL: string,
  store: string,
): Promise<void> {
  const { session, worker } = connection();
  try {
    const client = await create(session, {
      backend: { url: backendURL },
      storage: {
        location: { directory: `atch-records-${crypto.randomUUID()}` },
        singleConnection: false,
      },
      deviceSync: false,
      allowOffline: false,
      registration: { auto: true },
      attachments: { allowPrivateNetwork: true },
    });
    const attachments = client.attachments;
    const projection = currentProjection();
    for (const [
      index,
      [recorded, category, retryable],
    ] of FAILURE_TABLE.entries()) {
      const error = await thrownError(
        sdkConformanceAttachmentError(
          session,
          lowerAttachmentFailure(recorded, projection),
        ).catch((thrown: unknown) => {
          throw publicError(thrown);
        }),
      );
      same(error.attachmentFailure, recorded, `thrown ${recorded.cause}`);
      equal(error.details.category, category, recorded.cause);
      equal(error.details.retryable, retryable, recorded.cause);
      const pending = await attachments.create(bytesSource(`record ${index}`));
      await pending.sdkConformanceFail(recorded);
      same(
        await pending.status(),
        { kind: "failed", value: recorded },
        `status ${recorded.cause}`,
      );
    }
    const causes = new Set(FAILURE_TABLE.map(([recorded]) => recorded.cause));
    equal(causes.size, 21, "causes");
    const kinds = new Set(
      FAILURE_TABLE.flatMap(([recorded]) => recorded.credentialKind ?? []),
    );
    equal(kinds.size, 4, "credential kinds");

    // A terminal backend rejection is not sent again.
    const events = await client.events(attachmentFilter());
    const rejected = await attachments.create(bytesSource("rejected"));
    await rejected.sdkConformanceFail(failure("backendRejected"));
    for (let attempt = 0; attempt < 2; attempt += 1)
      same(
        (await thrownError(rejected.upload())).attachmentFailure,
        failure("backendRejected"),
        `upload ${attempt} after a terminal rejection`,
      );
    same(
      await rejected.status(),
      { kind: "failed", value: failure("backendRejected") },
      "terminal rejection status",
    );
    same(await drain(client, events), [], "a terminal rejection was resent");

    // The worker's death fails a call in flight and a waiting reader with
    // the worker failure. Later calls fail closed; held values stay readable.
    const held = await attachments.create(bytesSource("held"));
    const remote = held.remoteAttachment;
    const inFlight = attachments.download({
      ...remote,
      url: `${store}/hang`,
      contentDigest: "11".repeat(32),
    });
    void inFlight.catch(() => {});
    const started = await within(events.next(), "download start");
    equal(started.value?.kind, "attachmentDownloadStarted", "download start");
    const waiting = events.next();
    void waiting.catch(() => {});
    await bridgeTestPanic(session).catch(() => {});
    await rejectsWorkerDeath(inFlight, "download in flight");
    await rejectsWorkerDeath(waiting, "waiting event reader");
    const laterCalls: Array<[string, () => Promise<unknown>]> = [
      ["create", () => attachments.create(bytesSource("late"))],
      ["localPath", () => attachments.localPath(remote)],
      ["listPending", () => attachments.listPending()],
      ["status", () => held.status()],
      ["upload", () => held.upload()],
    ];
    for (const [label, call] of laterCalls)
      await rejectsClosed(call(), `${label} after the worker died`);
    equal(attachments.offered, true, "offered after the worker died");
    equal(client.attachments.offered, true, "attachments after the worker died");
    same(held.remoteAttachment, remote, "record after the worker died");
  } finally {
    worker.terminate();
  }
}
