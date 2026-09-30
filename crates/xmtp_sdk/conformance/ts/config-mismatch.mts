import assert from "node:assert/strict";
import { mkdtemp } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import * as sdk from "../../../../target/sdk-conformance/typescript-napi/index.ts";

/**
 * An offline build whose database is bound to another deployment re-checks
 * the backend before its first request, and the app gets the public
 * BackendMismatch code.
 */
// verifies: CONF-064, CONF-077
export async function checkConfigurationMismatch(
  signer: sdk.Signer,
  backend: sdk.BackendOptions,
): Promise<void> {
  const options: sdk.ClientOptions = {
    backend,
    storage: {
      location: {
        path: join(await mkdtemp(join(tmpdir(), "f6-mismatch-")), "client.db"),
      },
    },
    deviceSync: false,
  };
  const online = await sdk.Client.create(signer, options);
  const inboxId = online.inboxId;
  await online.conversations.sdkConformanceBindOtherDeployment();
  await online.end();

  const offline = await sdk.Client.build(
    await signer.identity(),
    { ...options, allowOffline: true },
    inboxId,
  );
  await assert.rejects(offline.inboxState(true), (error: unknown) => {
    assert.ok(
      error instanceof sdk.XmtpError.BackendMismatch,
      `expected BackendMismatch, got ${String(error)}`,
    );
    assert.equal(error.details.code, "BackendMismatch");
    assert.equal(error.details.category, "configuration");
    assert.equal(error.details.retryable, false);
    return true;
  });
  await offline.end();
}
