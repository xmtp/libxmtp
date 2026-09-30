// Attachment failure records in the conformance-featured
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
import { heldTransfer } from "../ts/transfer-control.mts";
import {
  attachmentFilter,
  bytesSource,
  drain,
  failure,
  same,
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

// Every transport discriminant and optional field. Rust owns the full policy table.
const FAILURE_TABLE: fx.AttachmentFailure[] = [
  failure("notOffered"),
  failure("tooLarge"),
  failure("sourceUnreadable"),
  failure("localStorage"),
  failure("stagedUnusable"),
  failure("connectionBlocked"),
  failure("credential", {
    credentialKind: "credentialRejected",
    missingScope: true,
  }),
  failure("credential", { credentialKind: "callbackFailed", retryable: true }),
  failure("credential", { credentialKind: "exhausted" }),
  failure("credential", { credentialKind: "missingCredential" }),
  failure("backendRejected"),
  failure("backendUnavailable"),
  failure("targetRejected", { httpStatus: 403 }),
  failure("network"),
  failure("insecureUrl"),
  failure("blockedAddress"),
  failure("tooManyRedirects"),
  failure("notFound", { httpStatus: 404 }),
  failure("httpStatus", { httpStatus: 408 }),
  failure("httpStatus", { httpStatus: 429 }),
  failure("httpStatus", { httpStatus: 503 }),
  failure("httpStatus", { httpStatus: 403 }),
  failure("malformed"),
  failure("digestMismatch"),
  failure("decryptionFailed"),
  failure("notAnAttachment"),
  failure("deleted"),
];

/**
 * Every cause and credential kind, thrown and recorded; no resend of a
 * terminal rejection.
 */
export async function checkAttachmentRecords(store: string): Promise<void> {
  const { session, worker } = connection();
  const held = await heldTransfer(store);
  try {
    const client = await create(session, {
      backend: { url: held.backend },
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
    for (const [index, recorded] of FAILURE_TABLE.entries()) {
      const error = await thrownError(
        sdkConformanceAttachmentError(
          session,
          lowerAttachmentFailure(recorded, projection),
        ).catch((thrown: unknown) => {
          throw publicError(thrown);
        }),
      );
      same(error.attachmentFailure, recorded, `thrown ${recorded.cause}`);
      if (recorded.cause === "credential") {
        equal(error.details.category, "callback", recorded.cause);
        equal(error.details.retryable, recorded.retryable, recorded.cause);
      }
      const pending = await attachments.create(bytesSource(`record ${index}`));
      await pending.sdkConformanceFail(recorded);
      same(
        await pending.status(),
        { kind: "failed", value: recorded },
        `status ${recorded.cause}`,
      );
    }
    const causes = new Set(FAILURE_TABLE.map((recorded) => recorded.cause));
    equal(causes.size, 21, "causes");
    const kinds = new Set(
      FAILURE_TABLE.flatMap((recorded) => recorded.credentialKind ?? []),
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

    same(
      await (await held.command("counts")).json(),
      { puts: 0, grants: 0, gets: 0 },
      "terminal rejection sent a request",
    );
    await events.return();
    await client.end();
  } finally {
    worker.terminate();
  }
}
