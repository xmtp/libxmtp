import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import * as B from "../../../../target/sdk-conformance/typescript-napi/xmtp_sdk.ts";
import {
  dispose,
  drained,
  lifetimeCycles,
  rawOptions,
  within,
} from "./callback-lifetime-support.mts";

export async function checkConstructorAdoption(backendURL: string) {
  const directory = await mkdtemp(join(tmpdir(), "xmtp-callback-adoption-"));
  try {
    for (const build of [false, true]) {
      for (const cancel of [false, true]) {
        for (let cycle = 0; cycle < lifetimeCycles; cycle++) {
          const path = join(directory, `${build}-${cancel}-${cycle}.db3`);
          const options = rawOptions(backendURL, {
            location: B.StorageLocation.Explicit.new({
              dbPath: path,
              attachmentsDir: `${path}.attachments`,
            }),
            singleConnection: true,
          });
          const signer = await B.generateLocalSigner();
          const identity = await signer.identity();
          if (build) {
            const seed = await B.Client.create(signer, options);
            await seed.end();
            dispose(seed);
          }
          const probe = await B.SdkConformanceConstructorProbe.open();
          const controller = new AbortController();
          const pending = (
            build
              ? probe.build(identity, options, undefined, {
                  signal: controller.signal,
                })
              : probe.create(signer, options, { signal: controller.signal })
          ).then(
            (value) => ({ status: "fulfilled", value }) as const,
            (reason: unknown) => ({ status: "rejected", reason }) as const,
          );
          try {
            await within(
              probe.waitForCompleted(),
              "constructor task completion",
            );
            assert.deepEqual(probe.state(), {
              storeOpenReported: false,
              clientCaptured: true,
              clientClosed: false,
              workersStopped: false,
              storeConnected: true,
            });
            if (cancel) {
              controller.abort();
              const result = await within(
                pending,
                "cancel before parent adoption",
              );
              assert.equal(result.status, "rejected");
              await within(
                probe.waitForCleanup(),
                "unadopted constructor cleanup",
              );
              assert.deepEqual(probe.state(), {
                storeOpenReported: true,
                clientCaptured: true,
                clientClosed: true,
                workersStopped: true,
                storeConnected: false,
              });
            } else {
              probe.release();
              const result = await within(pending, "constructor adoption");
              assert.equal(result.status, "fulfilled");
              assert.equal(probe.state().storeOpenReported, false);
              assert.equal(probe.state().storeConnected, true);
              await probe.endAdopted();
            }
          } finally {
            controller.abort();
            await probe.cleanup();
            dispose(probe);
            dispose(signer);
          }
          await drained();
        }
        console.log(
          `Node constructor lifetime: ${build ? "build" : "create"}, ${cancel ? "cancel before adoption" : "adopt"}, ${lifetimeCycles} cycles passed`,
        );
      }
    }
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
}

export async function checkConstructorFailure(backendURL: string) {
  const directory = await mkdtemp(join(tmpdir(), "xmtp-callback-failure-"));
  try {
    for (const build of [false, true])
      for (let cycle = 0; cycle < lifetimeCycles; cycle++) {
        const path = join(directory, `${build}-${cycle}.db3`);
        const options = rawOptions(backendURL, {
          location: B.StorageLocation.Explicit.new({
            dbPath: path,
            attachmentsDir: `${path}.attachments`,
          }),
          singleConnection: true,
        });
        const base = await B.generateLocalSigner();
        const probe = await B.SdkConformanceConstructorProbe.open();
        const badSigner: B.Signer = {
          identity: () => base.identity(),
          kind: () => Promise.reject(new B.SignerError.Failed()),
          sign: (request) => base.sign(request),
        };
        const pending = (
          build
            ? probe.build(await base.identity(), options, undefined)
            : probe.create(badSigner, options)
        ).then(
          () => ({ status: "fulfilled" }) as const,
          (reason: unknown) => ({ status: "rejected", reason }) as const,
        );
        try {
          await within(
            probe.waitForCompleted(),
            "failed constructor completion",
          );
          const state = probe.state();
          assert.equal(state.storeOpenReported, false);
          assert.equal(state.storeConnected, false);
          if (!build) {
            assert.equal(state.clientCaptured, true);
            assert.equal(state.clientClosed, true);
            assert.equal(state.workersStopped, true);
          }
          probe.release();
          assert.equal(
            (await within(pending, "failed constructor return")).status,
            "rejected",
          );
        } catch (error) {
          console.error(
            JSON.stringify(
              {
                build,
                cycle,
                state: probe.state(),
                tasks: B.sdkConformanceForeignCallCounts(),
                handles: B.sdkConformanceCallbackHandleCounts(),
              },
              (_key, value: unknown) =>
                typeof value === "bigint" ? value.toString() : value,
            ),
          );
          throw error;
        } finally {
          await probe.cleanup();
          dispose(probe);
          dispose(base);
        }
        await drained();
      }
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
  console.log(
    `Node constructor lifetime: clean create/build failure, ${lifetimeCycles} cycles passed`,
  );
}
