import { Client, type PublicIdentity } from "@xmtp/react-native-sdk";
import { NativeModules, Platform } from "react-native";

import { assert, equal } from "./assertions";
import { within } from "./async";
import { installedHarnessSha256 } from "./build-identity";
import type { JourneyControl } from "./journey";
import { claimLaunch } from "./launch";

/** A native cold background launch, followed by an external OS foreground action. */
export async function backgroundJourney(
  control: JourneyControl,
): Promise<string[]> {
  const native = NativeModules.XmtpRuntimeFixture as {
    processState(): Promise<"foreground" | "background">;
    initializationState?: string;
  };
  if (Platform.OS === "ios") {
    equal(
      native.initializationState,
      "background",
      "first React runtime and SDK must initialize in actual OS background",
    );
  }
  equal(
    await native.processState(),
    "background",
    "first SDK client must start in an actually backgrounded process",
  );
  const setup = await control.call<{
    identity: PublicIdentity;
    backendUrl: string;
    peerInboxId: string;
  }>("background/setup");
  const client = await Client.create(
    {
      identity: async () => setup.identity,
      kind: async () => ({ kind: "eoa" }),
      sign: async ({ text }) => ({
        kind: "ecdsa",
        value: new Uint8Array(
          (await control.call<{ bytes: number[] }>("sign", { text })).bytes,
        ),
      }),
    },
    {
      backend: { url: setup.backendUrl },
      deviceSync: false,
      storage: { location: "default", label: "cold-background" },
    },
  );
  try {
    const dm = await client.conversations.createDm(setup.peerInboxId);
    const snapshot = await dm.messageHistorySnapshot(0);
    const reader = dm.streamMessages({ from: snapshot.cursor });
    try {
      await within(reader.ready(), "background reader opens locally");
      let delivered = false;
      const next = reader.next().then((item) => {
        delivered = true;
        return item;
      });
      void next.catch(() => {});
      const sent = await control.call<{ messageId: string }>(
        "background/send",
        { conversationId: dm.id },
      );
      // The peer waits two seconds after publication. The driver does not bring
      // the app forward until this runtime explicitly requests that OS action.
      await control.call("background/observe");
      equal(
        await native.processState(),
        "background",
        "observation window must remain in the OS background",
      );
      const stats = await client.diagnostics.apiStatistics();
      equal(
        stats.subscribe + stats.subscribeStatic,
        0n,
        "initial native background seed must prevent every subscription before foregrounding",
      );
      equal(
        delivered,
        false,
        "live message must wait for native foreground/resume",
      );
      await control.call("background/foreground");
      const message = await within(
        next,
        "native foreground notification resumes existing stream",
      );
      assert(
        !message.done,
        "foregrounded reader must deliver the peer message",
      );
      equal(
        message.value.id,
        sent.messageId,
        "foreground/resume must deliver the exact independently published message",
      );
      equal(
        await native.processState(),
        "foreground",
        "driver must really foreground the process",
      );
      return [
        "fresh native SDK initialized in OS background starts no subscriptions; OS foreground resumes the same reader and receives the peer message",
      ];
    } finally {
      await reader.end();
    }
  } finally {
    await client.end();
  }
}

/** Android registers this as a genuine Headless JS task in its fixture entry. */
export async function runBackgroundTask({
  url,
}: {
  url: string;
}): Promise<void> {
  if (!claimLaunch(url)) return;
  const params = new Map(
    url
      .slice(url.indexOf("?") + 1)
      .split("&")
      .map((part) => {
        const [key, value = ""] = part.split("=");
        return [decodeURIComponent(key!), decodeURIComponent(value)];
      }),
  );
  const endpoint = params.get("control");
  const token = params.get("token");
  assert(
    endpoint && token,
    "headless fixture requires its authenticated control endpoint",
  );
  const control: JourneyControl = {
    async call<T>(operation: string, input?: unknown): Promise<T> {
      const response = await fetch(`${endpoint}/${operation}`, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          Authorization: `Bearer ${token}`,
        },
        body: JSON.stringify(input ?? {}),
      });
      if (!response.ok)
        throw new Error(
          `control ${operation} failed with HTTP ${response.status}`,
        );
      return (await response.json()) as T;
    },
  };
  try {
    assert(
      (globalThis as typeof globalThis & { HermesInternal?: unknown })
        .HermesInternal,
      "headless fixture must execute real Hermes",
    );
    const assertions = await backgroundJourney(control);
    await control.call("result", {
      status: "passed",
      platform: Platform.OS,
      mode: "background",
      harnessSha256: installedHarnessSha256,
      phases: ["cold-background"],
      assertions,
    });
  } catch (error) {
    await control.call("result", {
      status: "failed",
      platform: Platform.OS,
      mode: "background",
      harnessSha256: installedHarnessSha256,
      message: error instanceof Error ? error.message : String(error),
    });
    throw error;
  }
}
