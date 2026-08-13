import { once } from "node:events";
import { setTimeout } from "node:timers/promises";
import { describe, expect, it, vi } from "vitest";
import { Agent } from "@/core/Agent";
import { createSigner, createUser } from "@/user/User";
import { createClient } from "@/util/test";

const PROXY_NAME = "node-go";
// libxmtp's dev backend maps the toxiproxy API to host port 8474
const TOXIPROXY_API = "http://localhost:8474";
const TOXIPROXY_PORT = "6010";

export async function enableBackend(enabled: boolean) {
  const res = await fetch(`${TOXIPROXY_API}/proxies/${PROXY_NAME}`, {
    method: "POST",
    body: JSON.stringify({
      name: PROXY_NAME,
      listen: `[::]:${TOXIPROXY_PORT}`,
      upstream: "node:5556",
      enabled,
    }),
  });
  if (!res.ok) throw new Error(`Failed to toggle proxy: ${await res.text()}`);
}

export async function createToxicAgent() {
  await enableBackend(true);
  return Agent.create(createSigner(createUser()), {
    env: "local",
    apiUrl: `http://localhost:${TOXIPROXY_PORT}`,
    dbPath: null,
    disableDeviceSync: true,
  });
}

describe("Agent reconnect", () => {
  it("should reconnect after a mid-stream disconnect", async () => {
    const agent = await createToxicAgent();
    await agent.start();

    const reconnected = once(agent, "start", {
      signal: AbortSignal.timeout(10000),
    });

    await enableBackend(false);
    await setTimeout(5000);
    await enableBackend(true);

    await reconnected;
    await agent.stop();
  });

  it("should reconnect when start() fails initially", async () => {
    const agent = await createToxicAgent();

    await enableBackend(false);

    const started = once(agent, "start", {
      signal: AbortSignal.timeout(10000),
    });
    void agent.start();

    await setTimeout(5000);
    await enableBackend(true);

    await started;
    await agent.stop();
  });

  it("should emit unhandledError on stream disconnect", async () => {
    const agent = await createToxicAgent();
    await agent.start();

    const errored = once(agent, "unhandledError", {
      signal: AbortSignal.timeout(10000),
    });

    await enableBackend(false);

    const [error] = (await errored) as [unknown];
    expect(error).toBeInstanceOf(Error);

    await agent.stop();
  });
});

// End-to-end regression for the single-recovery-owner fix: retryOnFail: false
// plus Agent-owned recovery/teardown. Complements node-sdk's terminal/silent
// intentional-close change (PR #3978), which removes spurious close errors.
//
// "exactly one conversation stream and one message stream" is not observable
// from outside the client, so we assert its consequence: across repeated
// disconnect/reconnect cycles each message id reaches exactly one handler
// invocation. A leaked/zombie stream would re-deliver, so a stable
// one-delivery-per-id count is the practical proxy.
describe("Agent stream recovery regression", () => {
  it("delivers each message exactly once across recovery cycles", async () => {
    const agent = await createToxicAgent();
    const deliveries = new Map<string, number>();
    agent.on("text", (ctx) => {
      const id = ctx.message.id;
      deliveries.set(id, (deliveries.get(id) ?? 0) + 1);
    });
    // Disconnects surface as unhandledError; absorb so they don't fail the run.
    agent.on("unhandledError", () => {});

    await agent.start();

    // The sender connects directly (not through the toxic proxy) so it can
    // reach the same backend node the agent's proxy fronts.
    const sender = await createClient();
    const dm = await sender.conversations.createDm(agent.client.inboxId);

    const sendAndAwait = async (text: string) => {
      const id = await dm.sendText(text);
      await vi.waitFor(
        () => {
          expect(deliveries.get(id)).toBe(1);
        },
        { timeout: 45000, interval: 250 },
      );
      return id;
    };

    await sendAndAwait("before-cycles");

    for (let cycle = 0; cycle < 2; cycle++) {
      const reconnected = once(agent, "start", {
        signal: AbortSignal.timeout(45000),
      });
      await enableBackend(false);
      await setTimeout(3000);
      await enableBackend(true);
      await reconnected;

      await sendAndAwait(`after-cycle-${cycle}`);
    }

    // Let any zombie/duplicate deliveries surface before asserting.
    await setTimeout(3000);

    for (const [id, count] of deliveries) {
      expect(count, `message ${id} delivered ${count} times`).toBe(1);
    }
    // Sanity: we actually exercised the initial send plus each cycle.
    expect(deliveries.size).toBeGreaterThanOrEqual(3);

    await agent.stop();
    // Generous budget: two recovery cycles plus message round-trips, which can
    // be slow when the shared backend is under load.
  }, 240000);

  it("stop() during pending recovery yields no further deliveries or errors", async () => {
    const agent = await createToxicAgent();
    const onText = vi.fn();
    const errors: unknown[] = [];
    agent.on("text", onText);
    agent.on("unhandledError", (error) => errors.push(error));

    await agent.start();

    // Register the listener before disconnecting so we can't miss the error
    // that fires while the proxy is being disabled.
    const recoveryStarted = once(agent, "unhandledError", {
      signal: AbortSignal.timeout(45000),
    });
    await enableBackend(false);
    await recoveryStarted;
    await setTimeout(500);

    const deliveriesAtStop = onText.mock.calls.length;
    const errorsAtStop = errors.length;

    await agent.stop();

    // Restore the backend: a zombie recovery would re-create streams here.
    await enableBackend(true);
    await setTimeout(3000);

    // A stopped agent must not receive newly sent messages.
    const sender = await createClient();
    const dm = await sender.conversations.createDm(agent.client.inboxId);
    await dm.sendText("after-stop");
    await setTimeout(3000);

    expect(onText.mock.calls.length, "no deliveries after stop()").toBe(
      deliveriesAtStop,
    );
    expect(errors.length, "no new errors after stop()").toBe(errorsAtStop);
  });
});
