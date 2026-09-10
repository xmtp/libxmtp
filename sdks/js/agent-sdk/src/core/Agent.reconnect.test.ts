import { once } from "node:events";
import { setTimeout } from "node:timers/promises";
import { describe, expect, it, vi } from "vitest";
import { Agent } from "@/core/Agent";
import { createSigner, createUser } from "@/user/User";
import { createClient } from "@/util/test";

const PROXY_NAME = "backend";
const TOXIPROXY_API = "http://localhost:8474";
const TOXIPROXY_PORT = "6010";

const DELIVERY_WAIT = { timeout: 30_000, interval: 100 };
// The transport can wait 30 seconds plus up to 30 seconds of jitter before
// its next reconnect attempt. Leave time for that attempt and catch-up.
const RECOVERY_WAIT = { timeout: 75_000, interval: 100 };

// Set transport deadlines before the first client reads them.
process.env.XMTP_GRPC_KEEPALIVE_INTERVAL_SECS = "10";
process.env.XMTP_GRPC_KEEPALIVE_TIMEOUT_SECS = "10";

async function proxyRequest(path: string, method: string, body?: object) {
  const response = await fetch(
    `${TOXIPROXY_API}/proxies/${PROXY_NAME}${path}`,
    {
      method,
      headers: { "Content-Type": "application/json" },
      body: body === undefined ? undefined : JSON.stringify(body),
    },
  );
  if (!response.ok) {
    throw new Error(`Proxy request failed: ${await response.text()}`);
  }
}

export async function enableBackend(enabled: boolean) {
  await proxyRequest("", "POST", { enabled });
}

async function blackHole(enabled: boolean) {
  if (enabled) {
    await proxyRequest("/toxics", "POST", {
      name: "agent-black-hole",
      type: "timeout",
      stream: "downstream",
      toxicity: 1,
      attributes: { timeout: 0 },
    });
  } else {
    await proxyRequest("/toxics/agent-black-hole", "DELETE");
  }
}

export async function createToxicAgent() {
  await enableBackend(true);
  return Agent.create(createSigner(createUser()), {
    backendUrl: `http://localhost:${TOXIPROXY_PORT}`,
    env: "local",
    dbPath: null,
    disableDeviceSync: true,
  });
}

describe("Agent reconnect", () => {
  it.each(["disconnect", "black hole"] as const)(
    "should reconnect and resume in order after a %s without closing or errors",
    async (fault) => {
      const agent = await createToxicAgent();
      const sender = await createClient();
      const received: string[] = [];
      const onError = vi.fn();
      const onStart = vi.fn();
      const onStop = vi.fn();
      agent.on("unhandledError", onError);
      agent.on("start", onStart);
      agent.on("stop", onStop);
      agent.on("conversation", (context) => {
        received.push(context.conversation.id);
      });
      const createConversation = () =>
        sender.conversations.createGroup([agent.client.inboxId]);
      let faultActive = false;
      try {
        await agent.start();
        const first = await createConversation();
        await expect.poll(() => received, DELIVERY_WAIT).toEqual([first.id]);

        if (fault === "black hole") await blackHole(true);
        else await enableBackend(false);
        faultActive = true;
        const missed = await createConversation();
        // Let both keepalive deadlines expire while the fault is active.
        await setTimeout(fault === "black hole" ? 30_000 : 5_000);
        expect(received).toEqual([first.id]);
        expect(onStart).toHaveBeenCalledTimes(1);
        expect(onStop).not.toHaveBeenCalled();
        expect(onError).not.toHaveBeenCalled();

        if (fault === "black hole") await blackHole(false);
        else await enableBackend(true);
        faultActive = false;
        await expect
          .poll(() => received, RECOVERY_WAIT)
          .toEqual([first.id, missed.id]);
        const after = await createConversation();
        await expect
          .poll(() => received, DELIVERY_WAIT)
          .toEqual([first.id, missed.id, after.id]);
        expect(onStart).toHaveBeenCalledTimes(1);
        expect(onStop).not.toHaveBeenCalled();
        expect(onError).not.toHaveBeenCalled();
      } finally {
        if (faultActive && fault === "black hole") await blackHole(false);
        await enableBackend(true);
        await agent.stop();
      }
      expect(onStop).toHaveBeenCalledTimes(1);
    },
  );

  it("should reconnect when start() fails initially", async () => {
    const agent = await createToxicAgent();
    try {
      await enableBackend(false);
      const started = once(agent, "start", {
        signal: AbortSignal.timeout(10_000),
      });
      void agent.start();
      await setTimeout(5_000);
      await enableBackend(true);
      await started;
    } finally {
      await enableBackend(true);
      await agent.stop();
    }
  });
});
