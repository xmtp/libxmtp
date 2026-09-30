import { expect, test } from "vitest";

import {
  checkAttachmentEnd,
  checkEventEnd,
  checkAttachmentWorkerDeath,
  checkAttachmentSourceShape,
} from "./attachment-lifetime.chromium";
import {
  checkAttachmentFailures,
  checkAttachmentFlow,
  checkAttachmentSettings,
} from "./attachments.chromium";
import { checkAttachmentRecords } from "./attachments.records.chromium";
import { checkMetadataFields } from "./metadata.chromium";
import { checkStorageLayout } from "./storage.layout.chromium";
import { runBrowserBridgeConformance } from "./suite.chromium";
import { checkRealWasmTrap } from "./suite.panic.chromium";

declare const __XMTP_BACKEND_URL__: string;
declare const __XMTP_S3_BASE_URL__: string | undefined;
declare const __SDK_FIXTURE_URL__: string;

test("browser bridge scenarios 1 to 11: readers in 7, events in 8, catchUpToLive", async () => {
  const results = await runBrowserBridgeConformance(__XMTP_BACKEND_URL__).catch(
    (error: unknown) => {
      console.error("browser conformance failed", error);
      throw error;
    },
  );
  for (const result of results) console.log(result);
  for (const scenario of [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]) {
    expect(
      results.some((line) => line.startsWith(`scenario ${scenario}:`)),
    ).toBe(true);
  }
  expect(results.filter((line) => line.startsWith("PENDING"))).toHaveLength(0);
  expect(results.filter((line) => line.startsWith("smoke:"))).toHaveLength(5);
}, 180_000);

test("real WASM trap settles pending bridge calls", checkRealWasmTrap, 30_000);

test(
  "storage layout: labelled, unsafe, and explicit locations reopen offline",
  () => checkStorageLayout(__SDK_FIXTURE_URL__),
  90_000,
);
test(
  "attachments: configuration and 64-bit options",
  () =>
    checkAttachmentSettings(
      __XMTP_BACKEND_URL__,
      __XMTP_S3_BASE_URL__ ?? "http://127.0.0.1:9067/attachments",
    ),
  60_000,
);
test(
  "attachments: upload, reopen, resume, download, and delete",
  () => checkAttachmentFlow(__XMTP_BACKEND_URL__, __SDK_FIXTURE_URL__),
  120_000,
);
test(
  "attachments: real failures carry one record",
  () => checkAttachmentFailures(__XMTP_BACKEND_URL__, __SDK_FIXTURE_URL__),
  90_000,
);
test(
  "attachments: end waits for an upload; calls fail closed",
  () => checkAttachmentEnd(__SDK_FIXTURE_URL__),
  120_000,
);
test(
  "attachments: every cause and credential kind in both forms",
  () => checkAttachmentRecords(__SDK_FIXTURE_URL__),
  120_000,
);
test(
  "metadata fields and profiles",
  () => checkMetadataFields(__XMTP_BACKEND_URL__),
  120_000,
);

import {
  checkWorkerAdmission,
  type AdmissionCase,
} from "./reader.admission.chromium";
for (const mode of [
  "large-cursor",
  "restored-peer",
  "admitted",
  "cancel",
  "owner-end",
  "overlap-end",
  "end-fails",
  "callback-throw",
  "callback-reject",
] satisfies AdmissionCase[]) {
  test(
    `real WASM reader admission: ${mode}`,
    () => checkWorkerAdmission(__XMTP_BACKEND_URL__, mode),
    90_000,
  );
}

test(
  "events: client end settles a held worker reply",
  () => checkEventEnd(__XMTP_BACKEND_URL__),
  60_000,
);
test(
  "attachments: cancelled binding waiter retains storage until PUT completes",
  () => checkAttachmentEnd(__SDK_FIXTURE_URL__, true),
  120_000,
);
test(
  "attachments: package worker death releases storage without replay",
  () => checkAttachmentWorkerDeath(__SDK_FIXTURE_URL__),
  90_000,
);
test(
  "attachments: malformed sources have no OPFS side effects",
  () => checkAttachmentSourceShape(__XMTP_BACKEND_URL__),
  60_000,
);
